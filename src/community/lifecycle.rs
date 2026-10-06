//! Server time is authoritative; client clocks cannot extend the operating window.
use super::{
    model as m,
    store::{Records, Result},
};
use serde_json::json;
pub const UNLIMITED_AT: i64 = 1_791_990_000; // 2026-10-15 00:00 JST
pub const STOP_AT: i64 = 1_798_729_200; // 2027-01-01 00:00 JST
pub fn stopped(at: i64) -> bool {
    at >= STOP_AT
}
pub fn charge(r: &mut Records, user: &str, at: i64) -> Result<()> {
    if stopped(at) {
        return Err("サービスの提供期間が終了しました".into());
    }
    if at >= UNLIMITED_AT {
        return Ok(());
    }
    let day = chrono::DateTime::from_timestamp(at + 9 * 3600, 0)
        .ok_or("日時が不正です")?
        .format("%Y-%m-%d")
        .to_string();
    for (key, limit) in [(format!("{day}:{user}"), 10), (day.clone(), 100)] {
        if m::get(r, "ai_quota", &key)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            >= limit
        {
            return Err("本日の生成AI上限に達しました".into());
        }
    }
    for key in [format!("{day}:{user}"), day] {
        let n = m::get(r, "ai_quota", &key)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        m::put(r, "ai_quota", &key, json!(n + 1));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_boundaries_and_no_unlimited_counter_writes() {
        assert_eq!(
            chrono::DateTime::from_timestamp(UNLIMITED_AT, 0)
                .unwrap()
                .to_rfc3339(),
            "2026-10-14T15:00:00+00:00"
        );
        assert_eq!(
            chrono::DateTime::from_timestamp(STOP_AT, 0)
                .unwrap()
                .to_rfc3339(),
            "2026-12-31T15:00:00+00:00"
        );
        let mut r = Records::new();
        for _ in 0..10 {
            charge(&mut r, "u", UNLIMITED_AT - 1).unwrap();
        }
        assert!(charge(&mut r, "u", UNLIMITED_AT - 1).is_err());
        let before = r.clone();
        for _ in 0..150 {
            charge(&mut r, "u", UNLIMITED_AT).unwrap();
        }
        assert_eq!(r, before);
        assert!(charge(&mut r, "u", STOP_AT - 1).is_ok());
        assert!(charge(&mut r, "u", STOP_AT).is_err());
    }
}
