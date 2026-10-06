//! Durable application-to-cloud delivery. Enqueue in the event's SQLite transaction.
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SCHEMA: &str = include_str!("../assets/delivery.sql");
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub event_id: String,
    pub tenant: String,
    pub owner: String,
    pub entity_id: String,
    pub name: String,
    pub place: String,
    pub kind: String,
    pub time: String,
    pub lat: f64,
    pub lon: f64,
    pub version: i64,
}
impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.tenant != "studio"
            || self.owner != "creator-local"
            || !self.entity_id.starts_with("spid_")
            || self.entity_id.len() > 64
            || Uuid::parse_str(self.entity_id.trim_start_matches("spid_")).is_err()
            || Uuid::parse_str(&self.event_id).is_err()
            || self.version < 1
            || self.name.trim().is_empty()
            || self.name.chars().count() > 100
            || self.place.trim().is_empty()
            || self.place.chars().count() > 100
            || !["ART", "MUSIC", "WALK", "AR"].contains(&self.kind.as_str())
            || !self.lat.is_finite()
            || !self.lon.is_finite()
            || self.lat.abs() > 90.
            || self.lon.abs() > 180.
            || self.time.len() != 16
            || self.name.contains('\0')
            || self.place.contains('\0')
        {
            return Err("invalid delivery snapshot".into());
        }
        Ok(())
    }
    pub fn hash(&self) -> Result<String> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }
}

pub fn enqueue(c: &Connection, entity: &str) -> Result<()> {
    let snapshot = c.query_row("SELECT id,name,place,kind,time,lat,lon,version,tenant,owner FROM events WHERE id=?1 AND tenant='studio'", [entity], |r| Ok(Snapshot {
        schema_version: 1, event_id: Uuid::new_v4().to_string(), entity_id: r.get(0)?, name:r.get(1)?, place:r.get(2)?, kind:r.get(3)?, time:r.get(4)?, lat:r.get(5)?, lon:r.get(6)?, version:r.get(7)?, tenant:r.get(8)?, owner:r.get(9)?
    }))?;
    snapshot.validate()?;
    c.execute("INSERT INTO managed_deliveries(event_id,entity_id,entity_version,payload) VALUES(?1,?2,?3,?4) ON CONFLICT(entity_id,entity_version) DO NOTHING", params![snapshot.event_id,snapshot.entity_id,snapshot.version,serde_json::to_string(&snapshot)?])?;
    Ok(())
}

pub struct Claim {
    pub event_id: String,
    pub payload: String,
    pub lease: String,
    pub attempts: i64,
}

pub fn claim(c: &mut Connection, now: i64) -> Result<Option<Claim>> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("UPDATE managed_deliveries SET status='DEAD',lease_token=NULL,lease_until=NULL,last_error='lease attempts exhausted' WHERE status='LEASED' AND lease_until<=?1 AND attempts>=5", [now])?;
    let item = tx.query_row("SELECT event_id,payload,attempts FROM managed_deliveries WHERE (status IN ('QUEUED','RETRY') AND next_attempt<=?1) OR (status='LEASED' AND lease_until<=?1 AND attempts<5) ORDER BY rowid LIMIT 1", [now], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).optional()?;
    let Some((event_id, payload, attempts)) = item else {
        tx.commit()?;
        return Ok(None);
    };
    let lease = Uuid::new_v4().to_string();
    tx.execute("UPDATE managed_deliveries SET status='LEASED',attempts=attempts+1,lease_token=?1,lease_until=?2 WHERE event_id=?3",params![lease,now+600,event_id])?;
    tx.commit()?;
    Ok(Some(Claim {
        event_id,
        payload,
        lease,
        attempts: attempts + 1,
    }))
}

pub fn finish(c: &mut Connection, item: &Claim, outcome: &str, now: i64) -> Result<()> {
    if !["DELIVERED", "SUPERSEDED"].contains(&outcome) {
        return Err("invalid outcome".into());
    }
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !crate::audit::valid(&tx) {
        return Err("audit integrity failed; local acknowledgment blocked".into());
    }
    if tx.execute("UPDATE managed_deliveries SET status=?1,lease_token=NULL,lease_until=NULL,last_error=NULL,delivered_at=?2 WHERE event_id=?3 AND status='LEASED' AND lease_token=?4",params![outcome,now,item.event_id,item.lease])? != 1 { return Err("delivery lease lost".into()); }
    if outcome == "DELIVERED" {
        // Only the currently stored version can become ACTIVE. A late older receipt cannot publish it.
        tx.execute("UPDATE events SET status='ACTIVE' WHERE tenant='studio' AND (id,version) IN (SELECT entity_id,entity_version FROM managed_deliveries WHERE event_id=?1)",[&item.event_id])?;
        tx.execute("UPDATE outbox SET status='DELIVERED' WHERE status='PENDING' AND entity IN (SELECT e.id FROM events e JOIN managed_deliveries d ON e.id=d.entity_id AND e.version=d.entity_version WHERE d.event_id=?1 AND e.tenant='studio')",[&item.event_id])?;
    }
    crate::audit::append(
        &tx,
        &format!("delivery.{}:R2", outcome.to_lowercase()),
        &item.event_id,
    )?;
    tx.commit()?;
    Ok(())
}
pub fn fail(c: &Connection, item: &Claim, now: i64, permanent: bool) -> Result<()> {
    let status = if permanent || item.attempts >= 5 {
        "DEAD"
    } else {
        "RETRY"
    };
    let delay = (15_i64 * 2_i64.pow(item.attempts.min(5) as u32 - 1)).min(300);
    // Never persist raw provider responses, credentials or payload fragments in error logs.
    c.execute("UPDATE managed_deliveries SET status=?1,lease_token=NULL,lease_until=NULL,next_attempt=?2,last_error=?3 WHERE event_id=?4 AND status='LEASED' AND lease_token=?5",params![status,now+delay,if permanent {"invalid snapshot"} else {"cloud delivery failed; inspect worker status"},item.event_id,item.lease])?;
    Ok(())
}
pub fn retry_dead(c: &Connection, event_id: &str) -> Result<()> {
    if c.execute("UPDATE managed_deliveries SET status='QUEUED',attempts=0,next_attempt=0,last_error=NULL WHERE event_id=?1 AND status='DEAD'",[event_id])? != 1 { return Err("dead delivery not found".into()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("../assets/schema.sql"))
            .unwrap();
        c.execute_batch(SCHEMA).unwrap();
        c.execute("INSERT INTO managed_deliveries(event_id,entity_id,entity_version,payload) VALUES('a','e',1,'{}')",[]).unwrap();
        c
    }
    #[test]
    fn crash_recovery_excludes_stale_ack() {
        let mut c = db();
        let a = claim(&mut c, 100).unwrap().unwrap();
        assert!(claim(&mut c, 101).unwrap().is_none());
        let b = claim(&mut c, 701).unwrap().unwrap();
        assert!(finish(&mut c, &a, "DELIVERED", 702).is_err());
        finish(&mut c, &b, "DELIVERED", 703).unwrap();
        assert!(crate::audit::valid(&c));
        assert!(claim(&mut c, 9999).unwrap().is_none());
    }
    #[test]
    fn bounded_retry_and_explicit_repair() {
        let mut c = db();
        for attempt in 1..=5 {
            let t = attempt * 1000;
            let a = claim(&mut c, t).unwrap().unwrap();
            fail(&c, &a, t, false).unwrap();
            assert!(claim(&mut c, t + 1).unwrap().is_none());
        }
        assert!(claim(&mut c, 10000).unwrap().is_none());
        retry_dead(&c, "a").unwrap();
        assert_eq!(claim(&mut c, 10001).unwrap().unwrap().attempts, 1);
    }
    #[test]
    fn old_receipt_cannot_activate_a_new_pending_version() {
        let mut c = db();
        c.execute("INSERT INTO events VALUES('e','test','Tokyo','ART','2026-10-01T18:00',35,139,2,'PENDING_GEO','studio','creator-local')",[]).unwrap();
        let old = claim(&mut c, 100).unwrap().unwrap();
        finish(&mut c, &old, "DELIVERED", 101).unwrap();
        assert_eq!(
            c.query_row("SELECT status FROM events", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "PENDING_GEO"
        );
        c.execute("INSERT INTO managed_deliveries(event_id,entity_id,entity_version,payload) VALUES('b','e',2,'{}')",[]).unwrap();
        let new = claim(&mut c, 102).unwrap().unwrap();
        finish(&mut c, &new, "DELIVERED", 103).unwrap();
        assert_eq!(
            c.query_row("SELECT status FROM events", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ACTIVE"
        );
    }
    #[test]
    fn queue_insert_rolls_back_with_event() {
        let mut c = db();
        {
            let tx = c.transaction().unwrap();
            tx.execute("INSERT INTO managed_deliveries(event_id,entity_id,entity_version,payload) VALUES('b','e',2,'{}')",[]).unwrap();
        }
        assert_eq!(
            c.query_row("SELECT count(*) FROM managed_deliveries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
