use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
pub type Result<T> = std::result::Result<T, String>;
pub type Records = BTreeMap<(String, String), Value>;
#[derive(Clone)]
pub enum Store {
    Local(Arc<Mutex<rusqlite::Connection>>),
    Spanner(reqwest::Client),
}
const DB: &str="https://spanner.googleapis.com/v1/projects/spatial-atlas-dev-260908-rn/instances/atlas-validation/databases/spatial-community";
impl Store {
    pub fn local(path: &str) -> Result<Self> {
        let c = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
        c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS community(scope TEXT NOT NULL,record_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(scope,record_id));").map_err(|e|e.to_string())?;
        Ok(Self::Local(Arc::new(Mutex::new(c))))
    }
    pub fn cloud() -> Result<Self> {
        Ok(Self::Spanner(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(25))
                .build()
                .map_err(|e| e.to_string())?,
        ))
    }
    pub async fn transact<T, F>(&self, mut f: F) -> Result<T>
    where
        F: FnMut(&mut Records) -> Result<T> + Send,
        T: Send,
    {
        match self {
            Self::Local(db) => {
                let mut c = db.lock().await;
                let tx = c.transaction().map_err(|e| e.to_string())?;
                let mut records = Records::new();
                {
                    let mut q = tx
                        .prepare("SELECT scope,record_id,payload FROM community")
                        .map_err(|e| e.to_string())?;
                    let rows = q
                        .query_map([], |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?,
                            ))
                        })
                        .map_err(|e| e.to_string())?;
                    for row in rows {
                        let (s, k, v) = row.map_err(|e| e.to_string())?;
                        records
                            .insert((s, k), serde_json::from_str(&v).map_err(|e| e.to_string())?);
                    }
                }
                let old = records.clone();
                let out = f(&mut records)?;
                super::graph::enqueue(&old, &mut records);
                check_size(&records)?;
                for ((s, k), v) in &records {
                    if old.get(&(s.clone(), k.clone())) != Some(v) {
                        tx.execute("INSERT INTO community VALUES(?1,?2,?3) ON CONFLICT(scope,record_id) DO UPDATE SET payload=excluded.payload",rusqlite::params![s,k,v.to_string()]).map_err(|e|e.to_string())?;
                    }
                }
                for (s, k) in old.keys() {
                    if !records.contains_key(&(s.clone(), k.clone())) {
                        tx.execute(
                            "DELETE FROM community WHERE scope=?1 AND record_id=?2",
                            rusqlite::params![s, k],
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(out)
            }
            Self::Spanner(client) => {
                let token = access_token(client).await?;
                for attempt in 0..4 {
                    let created =
                        post(client, &token, &format!("{DB}/sessions"), json!({})).await?;
                    let name = created["name"].as_str().ok_or("Spanner session missing")?;
                    if !name.starts_with("projects/spatial-atlas-dev-260908-rn/instances/atlas-validation/databases/spatial-community/sessions/"){return Err("Spanner session mismatch".into());}
                    let url = format!("https://spanner.googleapis.com/v1/{name}");
                    let result=async {
      let began=post(client,&token,&format!("{url}:beginTransaction"),json!({"options":{"readWrite":{}}})).await?;
      let tx=began["id"].as_str().ok_or("Spanner transaction missing")?;
      let result=async {
       let rows=post(client,&token,&format!("{url}:executeSql"),json!({"transaction":{"id":tx},"sql":"SELECT scope,record_id,payload FROM CommunityRecord"})).await?;
       if rows.get("resumeToken").is_some(){return Err("Dataset needs paginated storage adapter".into());}
       let mut records=Records::new();for row in rows["rows"].as_array().unwrap_or(&Vec::new()){records.insert((row[0].as_str().ok_or("invalid scope")?.into(),row[1].as_str().ok_or("invalid id")?.into()),serde_json::from_str(row[2].as_str().ok_or("invalid payload")?).map_err(|_|"invalid stored JSON")?);}
       let old=records.clone();let out=f(&mut records)?;super::graph::enqueue(&old,&mut records);check_size(&records)?;
       let mut changes=super::graph::mutations(&old,&records);for ((s,k),v) in &records{if old.get(&(s.clone(),k.clone()))!=Some(v){changes.push(json!({"insertOrUpdate":{"table":"CommunityRecord","columns":["scope","record_id","payload"],"values":[[s,k,v.to_string()]]}}));}}
       for (s,k) in old.keys(){if !records.contains_key(&(s.clone(),k.clone())){changes.push(json!({"delete":{"table":"CommunityRecord","keySet":{"keys":[[s,k]]}}}));}}
       if changes.is_empty(){post(client,&token,&format!("{url}:rollback"),json!({"transactionId":tx})).await?;}else{post(client,&token,&format!("{url}:commit"),json!({"transactionId":tx,"mutations":changes})).await?;}
       Ok(out)
      }.await;
      if result.is_err(){let _=post(client,&token,&format!("{url}:rollback"),json!({"transactionId":tx})).await;}
      result
     }.await;
                    let _ = client.delete(&url).bearer_auth(&token).send().await;
                    match result {
                        Err(ref e) if e == "ABORTED" && attempt < 3 => {
                            tokio::time::sleep(Duration::from_millis(50 * (attempt + 1))).await;
                        }
                        other => return other,
                    }
                }
                Err("transaction retry exhausted".into())
            }
        }
    }
}
fn check_size(r: &Records) -> Result<()> {
    if r.len() > 12000 || r.values().map(|v| v.to_string().len()).sum::<usize>() > 24_000_000 {
        Err("容量上限に達しました。管理者による容量計画の更新が必要です。".into())
    } else {
        Ok(())
    }
}
pub async fn access_token(client: &reqwest::Client) -> Result<String> {
    if let Ok(t) = std::env::var("ATLAS_ACCESS_TOKEN") {
        return Ok(t);
    }
    let r=client.get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token").header("Metadata-Flavor","Google").send().await.map_err(|_|"cloud identity unavailable")?;
    if !r.status().is_success() {
        return Err("cloud identity rejected".into());
    }
    let v: Value = r
        .json()
        .await
        .map_err(|_| "cloud identity response invalid")?;
    Ok(v["access_token"]
        .as_str()
        .ok_or("cloud token missing")?
        .into())
}
pub async fn post(client: &reqwest::Client, token: &str, url: &str, body: Value) -> Result<Value> {
    post_with_policy(client, token, url, &body, retry_safe(url, &body)).await
}
fn retry_safe(url: &str, body: &Value) -> bool {
    url.starts_with(&format!("{DB}/sessions/"))
        && ((url.ends_with(":executeSql")
            && body["sql"] == "SELECT scope,record_id,payload FROM CommunityRecord")
            || ((url.ends_with(":commit") || url.ends_with(":rollback"))
                && body["transactionId"].as_str().is_some()))
}
async fn post_with_policy(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    body: &Value,
    safe: bool,
) -> Result<Value> {
    for attempt in 0..3 {
        match post_once(client, token, url, body).await {
            Ok(v) => return Ok(v),
            Err((message, transient)) if safe && transient && attempt < 2 => {
                eprintln!("cloud_rpc_retry attempt={} reason={message}", attempt + 1);
                tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
            }
            Err((message, _)) => return Err(message),
        }
    }
    unreachable!()
}
async fn post_once(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    body: &Value,
) -> std::result::Result<Value, (String, bool)> {
    let r = client
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            (
                "cloud request failed".into(),
                e.is_timeout() || e.is_connect() || e.is_body(),
            )
        })?;
    let status = r.status();
    let content_type = r
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("missing")
        .to_owned();
    let bytes = r.bytes().await.map_err(|e| {
        let operation = url.rsplit(['/', ':']).next().unwrap_or("unknown");
        eprintln!("cloud_body_error operation={operation} status={status} type={content_type} detail={:?}", e.without_url());
        ("cloud response body unavailable".into(), true)
    })?;
    if matches!(status.as_u16(), 429 | 500 | 502 | 503 | 504) {
        return Err((
            format!("cloud service temporarily unavailable ({status})"),
            true,
        ));
    }
    let v: Value = serde_json::from_slice(&bytes).map_err(|e| {
        // Only transport metadata: never log credentials or record contents.
        let operation = url.rsplit(['/', ':']).next().unwrap_or("unknown");
        eprintln!("cloud_json_error operation={operation} status={status} type={content_type} bytes={} category={:?} line={} column={}", bytes.len(), e.classify(), e.line(), e.column());
        ("cloud response invalid".to_string(), e.is_eof())
    })?;
    if status.is_success() {
        Ok(v)
    } else if v["error"]["status"] == "ABORTED" {
        Err(("ABORTED".into(), false))
    } else {
        Err((format!("cloud service rejected request ({status})"), false))
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    #[test]
    fn retry_only_read_or_existing_transaction() {
        let url = format!("{DB}/sessions/test");
        assert!(retry_safe(
            &format!("{url}:commit"),
            &json!({"transactionId":"known"})
        ));
        assert!(!retry_safe(
            &format!("{url}:commit"),
            &json!({"singleUseTransaction":{}})
        ));
        assert!(!retry_safe(
            &format!("{url}:executeSql"),
            &json!({"sql":"DELETE FROM CommunityRecord"})
        ));
        assert!(!retry_safe(
            "https://aiplatform.googleapis.com/generateContent",
            &json!({})
        ));
    }
    #[tokio::test]
    async fn truncated_body_retries_same_payload() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for i in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = vec![0; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                requests.push(String::from_utf8_lossy(&buf[..n]).to_string());
                let response = if i == 0 {
                    "HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n{"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}"
                };
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let value = post_with_policy(
            &client,
            "test-only",
            &format!("http://{address}"),
            &json!({"transactionId":"same"}),
            true,
        )
        .await
        .unwrap();
        assert_eq!(value["ok"], true);
        let requests = task.await.unwrap();
        assert!(requests
            .iter()
            .all(|r| r.contains("\"transactionId\":\"same\"")));
    }
}
