//! Real HTTP -> SQLite queue -> Rust worker -> Spanner Graph + AlloyDB acceptance.
use reqwest::Client;
use rusqlite::Connection;
use spatial_atlas::{
    delivery::{Result, Snapshot},
    managed::Cloud,
};
use uuid::Uuid;
fn encode(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
async fn action(client: &Client, token: &str, op: &str, fields: &[(&str, &str)]) -> Result<()> {
    let mut body = format!("token={}&key={}&op={}", encode(token), Uuid::new_v4(), op);
    for (key, value) in fields {
        body.push_str(&format!("&{}={}", encode(key), encode(value)));
    }
    let response = client
        .post("http://127.0.0.1:8082/action")
        .header("cookie", format!("atlas_session={token}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?;
    if response.status() != reqwest::StatusCode::SEE_OTHER {
        return Err(format!("HTTP action {op} failed: {}", response.status()).into());
    }
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client.get("http://127.0.0.1:8082/").send().await?;
    let token = response
        .headers()
        .get("set-cookie")
        .ok_or("missing session")?
        .to_str()?
        .split(';')
        .next()
        .ok_or("cookie")?
        .strip_prefix("atlas_session=")
        .ok_or("cookie name")?
        .to_owned();
    let name = format!("検証 ' 星の庭 {}", Uuid::new_v4());
    action(
        &client,
        &token,
        "create",
        &[
            ("name", &name),
            ("place", "東京 ' Atelier"),
            ("kind", "ART"),
            ("time", "2026-10-01T18:00"),
            ("lat", "35.6984"),
            ("lon", "139.7731"),
        ],
    )
    .await?;
    let c = Connection::open(std::env::var("ATLAS_DB")?)?;
    let eid: String = c.query_row("SELECT id FROM events WHERE name=?1", [&name], |r| r.get(0))?;
    action(
        &client,
        &token,
        "propose",
        &[("id", &eid), ("version", "1"), ("time", "2026-10-02T19:00")],
    )
    .await?;
    let aid: String = c.query_row(
        "SELECT id FROM approvals WHERE entity=?1 AND status='PENDING'",
        [&eid],
        |r| r.get(0),
    )?;
    action(&client, &token, "approve", &[("id", &aid)]).await?;
    let count: i64 = c.query_row(
        "SELECT count(*) FROM managed_deliveries WHERE entity_id=?1",
        [&eid],
        |r| r.get(0),
    )?;
    if count != 2 {
        return Err("create and approval did not enqueue two versions".into());
    }
    println!("PASS HTTP create + approval -> two durable snapshots");
    let worker = std::env::current_exe()?.with_file_name("delivery_worker");
    if !std::process::Command::new(worker).status()?.success() {
        return Err("delivery worker failed".into());
    }
    let delivered: i64 = c.query_row(
        "SELECT count(*) FROM managed_deliveries WHERE entity_id=?1 AND status='DELIVERED'",
        [&eid],
        |r| r.get(0),
    )?;
    if delivered != 2 {
        return Err("not all versions delivered".into());
    }
    let status: String = c.query_row("SELECT status FROM events WHERE id=?1", [&eid], |r| {
        r.get(0)
    })?;
    if status != "ACTIVE" || !spatial_atlas::audit::valid(&c) {
        return Err("local activation or audit failed".into());
    }
    let payload: String = c.query_row(
        "SELECT payload FROM managed_deliveries WHERE entity_id=?1 AND entity_version=2",
        [&eid],
        |r| r.get(0),
    )?;
    let snapshot: Snapshot = serde_json::from_str(&payload)?;
    Cloud::from_env()?.verify_delivery(&snapshot).await?;
    println!("PASS canonical v2, Graph location, Outbox acknowledgment, PostGIS duplicate receipt");
    let page = client
        .get("http://127.0.0.1:8082/?tab=ops")
        .send()
        .await?
        .text()
        .await?;
    if !page.contains("反映済み 2") {
        return Err("UI does not show delivered count".into());
    }
    println!("PASS application operations UI shows two completed deliveries");
    Ok(())
}
