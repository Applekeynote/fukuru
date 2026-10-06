//! Run only during an explicitly started validation window. Never starts paid resources.
use spatial_atlas::{
    delivery::{self, Result, Snapshot},
    managed::{Cloud, Outcome},
};
use std::time::{SystemTime, UNIX_EPOCH};
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.as_slice() == ["--init-schema"] {
        Cloud::from_env()?.install_geometry_schema().await?;
        println!("geometry schema installed");
        return Ok(());
    }
    if !args.is_empty() {
        return Err("usage: delivery_worker [--init-schema]".into());
    }
    let path = std::env::var("ATLAS_DB")
        .map_err(|_| "ATLAS_DB must point to the application's database")?;
    // Do not create a new empty file when a path is misspelled.
    let mut db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch(delivery::SCHEMA)?;
    let cloud = Cloud::from_env()?;
    let mut failed = false;
    for _ in 0..10 {
        if !spatial_atlas::audit::valid(&db) {
            return Err("audit integrity failed; cloud delivery blocked".into());
        }
        let Some(item) = delivery::claim(&mut db, now())? else {
            break;
        };
        let snapshot = serde_json::from_str::<Snapshot>(&item.payload);
        let snapshot = match snapshot {
            Ok(s) if s.event_id == item.event_id && s.validate().is_ok() => s,
            _ => {
                delivery::fail(&db, &item, now(), true)?;
                failed = true;
                continue;
            }
        };
        match tokio::time::timeout(
            std::time::Duration::from_secs(480),
            cloud.deliver(&snapshot),
        )
        .await
        {
            Ok(Ok(outcome)) => {
                let state = if outcome == Outcome::Delivered {
                    "DELIVERED"
                } else {
                    "SUPERSEDED"
                };
                delivery::finish(&mut db, &item, state, now())?;
                println!("{} v{} {state}", snapshot.entity_id, snapshot.version);
            }
            error => {
                delivery::fail(&db, &item, now(), false)?;
                failed = true;
                match error {
                    Ok(Err(e)) => eprintln!("delivery failed: {e}"),
                    _ => eprintln!("delivery timed out"),
                }
            }
        }
    }
    if failed {
        return Err("one or more deliveries retained for retry or repair".into());
    }
    println!("delivery batch finished");
    Ok(())
}
