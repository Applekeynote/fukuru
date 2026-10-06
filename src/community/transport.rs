use chrono::DateTime;
use serde_json::{json, Value};

pub fn nearby_stations(raw:&Value,lat:f64,lon:f64)->Vec<Value>{
    let rad=|v:f64|v.to_radians();
    let mut stations:Vec<_>=raw.as_array().map(Vec::as_slice).unwrap_or(&[]).iter().filter_map(|s|{
        let slat=s["lat"].as_f64()?;let slon=s["lon"].as_f64()?;
        let h=((rad(slat-lat)/2.).sin()).powi(2)+rad(lat).cos()*rad(slat).cos()*((rad(slon-lon)/2.).sin()).powi(2);
        let distance=12_742_000.*h.sqrt().min(1.).asin();
        if !distance.is_finite()||distance>5000.{return None;}let mut station=s.clone();station["distance_m"]=json!(distance.round() as u64);Some(station)
    }).collect();stations.sort_by_key(|s|s["distance_m"].as_u64().unwrap_or(u64::MAX));stations
}

// Only publicly usable, still-valid information is returned. The key never leaves the server.
pub fn normalize(raw: &Value, now: i64) -> Value {
    let rows = raw.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut alerts: Vec<Value> = rows.iter().filter_map(|r| {
        let valid = DateTime::parse_from_rfc3339(r["dct:valid"].as_str()?).ok()?;
        let issued = DateTime::parse_from_rfc3339(r["dc:date"].as_str()?).ok()?;
        if valid.timestamp() <= now || issued.timestamp() > now + 60 || now - issued.timestamp() > 600 { return None; }
        let text = r["odpt:trainInformationText"]["ja"].as_str()
            .or_else(|| r["odpt:trainInformationStatus"]["ja"].as_str())?.trim();
        if text.is_empty() || text.contains("平常どおり") || text.contains("平常運転") { return None; }
        let railway = r["odpt:railway"].as_str()?.strip_prefix("odpt.Railway:")?;
        let operator = r["odpt:operator"].as_str()?.strip_prefix("odpt.Operator:")?;
        if text.chars().count()>500 { return None; }
        Some(json!({"railway":railway,"operator":operator,"text":text,"issued":issued.to_rfc3339(),"valid_until":valid.to_rfc3339()}))
    }).take(1000).collect();
    alerts.sort_by(|a,b| a["railway"].as_str().cmp(&b["railway"].as_str()));
    json!({"area":"capital","alerts":alerts,"checked_at":now,"source":"公共交通オープンデータセンター","coverage":"取得対象の首都圏路線のみ"})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn nearby_railways_follow_the_location_and_not_the_default_city(){
        let stations=json!([{"name":"Tokyo","lat":35.681,"lon":139.767},{"name":"Nagoya","lat":35.1709,"lon":136.8815},{"name":"Invalid","lat":95.0,"lon":0.0}]);
        let result=nearby_stations(&stations,35.171,136.882);assert_eq!(result.len(),1);assert_eq!(result[0]["name"],"Nagoya");assert!(result[0]["distance_m"].as_u64().unwrap()<100);
    }
    #[test]
    fn excludes_expired_and_unknown_status() {
        let now=1_800_000_000;
        let iso=|v| chrono::DateTime::from_timestamp(v,0).unwrap().to_rfc3339();
        let row=|text:&str,valid|json!({"dc:date":iso(now-60),"dct:valid":iso(valid),"odpt:railway":"odpt.Railway:TokyoMetro.Ginza","odpt:operator":"odpt.Operator:TokyoMetro","odpt:trainInformationText":{"ja":text}});
        let v=normalize(&json!([row("一部列車遅延",now+60),row("期限切れ",now),row("平常どおり運転しています。",now+60)]),now);
        assert_eq!(v["alerts"].as_array().unwrap().len(),1);
        assert_eq!(v["alerts"][0]["text"],"一部列車遅延");
    }
}
