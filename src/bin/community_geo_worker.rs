//! Only invoked by a bounded verification window; never starts a paid resource.
use serde_json::json;
use spatial_atlas::{
    community::{model as m, store::Store},
    managed::Cloud,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cloud = Cloud::from_env()?;
    if std::env::args().nth(1).as_deref() == Some("--init-schema") {
        cloud.install_community_geometry().await?;
        println!("COMMUNITY_GEO_SCHEMA_READY");
        return Ok(());
    }
    let store = Store::cloud()?;
    for _ in 0..100 {
        let ticket = m::uid();
        let now = m::now();
        let item = store
            .transact(|r| {
                let key = m::list(r, "geo_outbox")
                    .into_iter()
                    .find(|v| {
                        v["state"] != "DELIVERED"
                            && v["state"] != "HELD"
                            && v["lease"].as_i64().unwrap_or(0) < now
                    })
                    .and_then(|v| v["id"].as_str())
                    .map(str::to_owned);
                let Some(key) = key else { return Ok(None) };
                let v = r.get_mut(&("geo_outbox".into(), key)).unwrap();
                v["lease"] = json!(now + 180);
                v["ticket"] = json!(ticket);
                v["attempts"] = json!(v["attempts"].as_u64().unwrap_or(0) + 1);
                Ok(Some(v.clone()))
            })
            .await?;
        let Some(item) = item else {
            println!("COMMUNITY_GEO_BATCH_FINISHED");
            return Ok(());
        };
        let id = item["id"].as_str().unwrap();
        let result = cloud.community_geometry(&item["event"], id).await;
        store
            .transact(|r| {
                let v = r
                    .get_mut(&("geo_outbox".into(), id.into()))
                    .ok_or("missing outbox")?;
                if v["ticket"] != ticket {
                    return Err("lease superseded".into());
                }
                v["lease"] = json!(m::now() + 60);
                v["state"] = json!(if result.is_ok() {
                    "DELIVERED"
                } else if v["attempts"].as_u64().unwrap_or(0) >= 5 {
                    "HELD"
                } else {
                    "PENDING"
                });
                Ok(())
            })
            .await?;
        println!(
            "{id}: {}",
            result.as_deref().unwrap_or("RETAINED_FOR_RETRY")
        );
        result?;
    }
    Err("batch limit reached; pending records retained".into())
}
