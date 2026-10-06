use spatial_atlas::community::{demo_import::Manifest, model as m, store::Store};
#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2
        || args.len() > 3
        || args[0] != "--manifest"
        || args.get(2).is_some_and(|a| a != "--apply")
    {
        return Err("Usage: spatial_demo_import --manifest PATH [--apply]".into());
    }
    let manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(&args[1]).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    manifest.validate()?;
    let store = if std::env::var("ATLAS_STORE").as_deref() == Ok("spanner") {
        Store::cloud()?
    } else {
        Store::local(
            &std::env::var("ATLAS_COMMUNITY_DB")
                .map_err(|_| "Set ATLAS_COMMUNITY_DB for local import")?,
        )?
    };
    store
        .transact(|r| manifest.check_target(r).map(|_| ()))
        .await?;
    if args.get(2).is_none() {
        println!("DRY RUN: 46 disabled demo accounts; 705 demo events; no AI; no writes");
        return Ok(());
    }
    let mut added = 0;
    for offset in (0..manifest.events.len()).step_by(45) {
        added += store
            .transact(|r| manifest.apply_chunk(r, offset, 45))
            .await?;
        println!("Imported chunk {} / {}", (offset + 45).min(705), 705);
    }
    let (events, accounts, own) = store
        .transact(|r| {
            let owner = manifest.check_target(r)?;
            let es = m::list(r, "event");
            Ok((
                es.iter()
                    .filter(|e| e["seed_batch"] == manifest.batch)
                    .count(),
                m::list(r, "account")
                    .iter()
                    .filter(|a| a["seed_batch"] == manifest.batch)
                    .count(),
                es.iter()
                    .filter(|e| e["seed_batch"] == manifest.batch && e["owner"] == owner)
                    .count(),
            ))
        })
        .await?;
    if (events, accounts, own) != (705, 46, 15) {
        return Err("Import verification failed".into());
    }
    println!("VERIFIED: events={events}, disabled_accounts={accounts}, rynat_events={own}, added={added}");
    Ok(())
}
