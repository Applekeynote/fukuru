use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
pub fn append(c: &Connection, action: &str, entity: &str) -> rusqlite::Result<()> {
    let prev: String = c
        .query_row(
            "SELECT hash FROM audit ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    let hash = format!("{:x}", Sha256::digest(format!("{prev}|{action}|{entity}")));
    c.execute(
        "INSERT INTO audit(action,entity,prev,hash) VALUES(?1,?2,?3,?4)",
        params![action, entity, prev, hash],
    )?;
    Ok(())
}
pub fn valid(c: &Connection) -> bool {
    let mut prev = String::new();
    let mut s = c
        .prepare("SELECT action,entity,prev,hash FROM audit ORDER BY seq")
        .unwrap();
    for row in s
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .unwrap()
    {
        let (a, e, p, h) = row.unwrap();
        if p != prev || h != format!("{:x}", Sha256::digest(format!("{p}|{a}|{e}"))) {
            return false;
        }
        prev = h
    }
    true
}
