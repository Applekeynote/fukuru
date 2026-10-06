//! Fixed-destination cloud adapter. No arbitrary SQL, URL or tool entry point.
use crate::delivery::{Result, Snapshot};
use reqwest::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;
pub const PROJECT: &str = "spatial-atlas-dev-260908-rn";
pub struct Cloud {
    client: Client,
    token: String,
}
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Delivered,
    Superseded,
}
impl Cloud {
    pub fn from_env() -> Result<Self> {
        if std::env::var("ATLAS_VERIFY_PROJECT").as_deref() != Ok(PROJECT) {
            return Err("explicit validation project required".into());
        }
        let token = std::env::var("ATLAS_ACCESS_TOKEN")?;
        if token.is_empty() {
            return Err("empty token".into());
        }
        Ok(Self {
            client: Client::builder().timeout(Duration::from_secs(45)).build()?,
            token,
        })
    }
    async fn post(&self, url: &str, body: Value) -> Result<Value> {
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .header("x-goog-user-project", PROJECT)
            .header("Accept", "application/json, text/event-stream")
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("Cloud HTTP {status}").into());
        }
        let text = response.text().await?;
        if let Ok(v) = serde_json::from_str(&text) {
            return Ok(v);
        }
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if v.get("result").is_some() || v.get("error").is_some() {
                        return Ok(v);
                    }
                }
            }
        }
        Err("unsupported Cloud response".into())
    }
    async fn sql(&self, statement: &str) -> Result<Value> {
        let r=self.post("https://alloydb.googleapis.com/mcp",json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"execute_sql","arguments":{"instance":format!("projects/{PROJECT}/locations/asia-northeast1/clusters/atlas-geo-validation/instances/primary"),"database":"postgres","sqlStatement":statement,"validateOnly":false}}})).await?;
        sql_result(r)
    }
    pub async fn install_geometry_schema(&self) -> Result<()> {
        self.sql(include_str!("../infra/alloy-delivery.sql"))
            .await?;
        Ok(())
    }
    pub async fn install_community_geometry(&self) -> Result<()> {
        self.sql(include_str!("../infra/community-geo.sql")).await?;
        Ok(())
    }
    pub async fn community_geometry(&self, event: &Value, delivery: &str) -> Result<String> {
        let encoded = hex(&serde_json::to_vec(event)?);
        let id = hex(delivery.as_bytes());
        let digest = format!("{:x}", Sha256::digest(event.to_string().as_bytes()));
        let r=self.sql(&format!("SELECT public.community_apply(convert_from(decode('{encoded}','hex'),'UTF8')::jsonb,convert_from(decode('{id}','hex'),'UTF8'),'{digest}') AS outcome")).await?;
        let outcome = r["sqlResults"][0]["rows"][0]["values"][0]["value"]
            .as_str()
            .ok_or("missing receipt")?;
        if !["APPLIED", "DUPLICATE", "SUPERSEDED"].contains(&outcome) {
            return Err("invalid receipt".into());
        }
        Ok(outcome.into())
    }
    async fn session(&self) -> Result<String> {
        let v=self.post(&format!("https://spanner.googleapis.com/v1/projects/{PROJECT}/instances/atlas-validation/databases/spatial/sessions"),json!({})).await?;
        let name = v["name"].as_str().ok_or("missing session")?;
        let prefix =
            format!("projects/{PROJECT}/instances/atlas-validation/databases/spatial/sessions/");
        if !name.starts_with(&prefix) {
            return Err("unexpected session destination".into());
        }
        Ok(format!("https://spanner.googleapis.com/v1/{name}"))
    }
    async fn query(&self, base: &str, tx: Option<&str>, sql: &str, params: Value) -> Result<Value> {
        let types = params
            .as_object()
            .ok_or("invalid query params")?
            .keys()
            .map(|k| (k.clone(), json!({"code":"STRING"})))
            .collect::<serde_json::Map<_, _>>();
        let mut body = json!({"sql":sql,"params":params,"paramTypes":types});
        if let Some(t) = tx {
            body["transaction"] = json!({"id":t});
        }
        self.post(&format!("{base}:executeSql"), body).await
    }
    async fn begin(&self, base: &str) -> Result<String> {
        let v = self
            .post(
                &format!("{base}:beginTransaction"),
                json!({"options":{"readWrite":{}}}),
            )
            .await?;
        Ok(v["id"].as_str().ok_or("missing transaction")?.to_owned())
    }
    async fn commit(&self, base: &str, tx: &str, mutations: Vec<Value>) -> Result<()> {
        self.post(
            &format!("{base}:commit"),
            json!({"transactionId":tx,"mutations":mutations}),
        )
        .await?;
        Ok(())
    }
    async fn rollback(&self, base: &str, tx: &str) {
        let _ = self
            .post(&format!("{base}:rollback"), json!({"transactionId":tx}))
            .await;
    }

    async fn publish(&self, base: &str, e: &Snapshot) -> Result<Option<Outcome>> {
        let tx = self.begin(base).await?;
        let result = async {
            let previous = self
                .query(
                    base,
                    Some(&tx),
                    "SELECT tenant_id,version FROM SpatialIdentity WHERE spatial_id=@id",
                    json!({"id":e.entity_id}),
                )
                .await?;
            let prior = rows(&previous)?.first().cloned();
            if let Some(ref p) = prior {
                if p[0] != e.tenant {
                    return Err("tenant conflict".into());
                }
            }
            let outbox = self
                .query(
                    base,
                    Some(&tx),
                    "SELECT payload,status FROM Outbox WHERE event_id=@id AND tenant_id=@tenant",
                    json!({"id":e.event_id,"tenant":e.tenant}),
                )
                .await?;
            if let Some(row) = rows(&outbox)?.first() {
                let payload: Snapshot =
                    serde_json::from_str(row[0].as_str().ok_or("invalid Outbox JSON")?)?;
                if &payload != e {
                    return Err("Outbox idempotency conflict".into());
                }
                if row[1] == "DELIVERED" {
                    return Ok(Some(Outcome::Delivered));
                }
                return Ok(None);
            }
            let old_version = prior.as_ref().map(|p| parse_version(&p[1])).transpose()?;
            if old_version.is_some_and(|v| v > e.version) {
                return Ok(Some(Outcome::Superseded));
            }
            if old_version == Some(e.version) {
                return Err("version exists without matching delivery".into());
            }
            let creator = linked_id("creator", &e.tenant, &e.owner);
            let place = linked_id("place", &e.tenant, &e.place);
            let mut mutations = vec![
                identity("insertOrUpdate", &creator, "CREATOR", e, 1, "ACTIVE", true),
                identity("insertOrUpdate", &place, "PLACE", e, 1, "ACTIVE", true),
                identity(
                    if prior.is_some() { "update" } else { "insert" },
                    &e.entity_id,
                    "EVENT",
                    e,
                    e.version,
                    "PENDING_GEO",
                    prior.is_none(),
                ),
            ];
            mutations.push(mutation(
                "insertOrUpdate",
                "SpatialRelationship",
                &["source_id", "target_id", "relation", "tenant_id"],
                json!([
                    [creator, e.entity_id, "CREATED", e.tenant],
                    [e.entity_id, place, "LOCATED_AT", e.tenant]
                ]),
            ));
            mutations.push(mutation(
                "insertOrUpdate",
                "EventDetails",
                &["spatial_id", "tenant_id", "details"],
                json!([[e.entity_id, e.tenant, serde_json::to_string(e)?]]),
            ));
            mutations.push(mutation(
                "insert",
                "Outbox",
                &[
                    "event_id",
                    "entity_id",
                    "tenant_id",
                    "event_type",
                    "schema_version",
                    "entity_version",
                    "idempotency_key",
                    "trace_id",
                    "payload",
                    "status",
                    "occurred_at",
                ],
                json!([[
                    e.event_id,
                    e.entity_id,
                    e.tenant,
                    "geometry.upsert",
                    "1",
                    e.version.to_string(),
                    format!("{}:{}", e.entity_id, e.version),
                    e.event_id,
                    serde_json::to_string(e)?,
                    "PENDING",
                    "spanner.commit_timestamp()"
                ]]),
            ));
            self.commit(base, &tx, mutations).await?;
            Ok(None)
        }
        .await;
        // Also release read locks on duplicate, superseded and error paths.
        self.rollback(base, &tx).await;
        result
    }
    async fn geometry(&self, e: &Snapshot) -> Result<Outcome> {
        // The API has no bind-parameter field. Hex-encode data; it cannot terminate a SQL literal.
        let encoded = hex(&serde_json::to_vec(e)?);
        let hash = e.hash()?;
        let response=self.sql(&format!("SELECT public.atlas_apply_delivery(convert_from(decode('{encoded}','hex'),'UTF8')::jsonb,'{hash}')::text AS receipt;")).await?;
        let s = response["sqlResults"][0]["rows"][0]["values"][0]["value"]
            .as_str()
            .ok_or("missing geometry receipt")?;
        let receipt: Value = serde_json::from_str(s)?;
        if receipt["event_id"] != e.event_id
            || receipt["version"] != e.version
            || receipt["receipt_count"] != 1
            || receipt["radius_match"] != true
        {
            return Err("geometry receipt failed".into());
        }
        match receipt["outcome"].as_str() {
            Some("APPLIED") => Ok(Outcome::Delivered),
            Some("SUPERSEDED") => Ok(Outcome::Superseded),
            _ => Err("unknown geometry outcome".into()),
        }
    }
    async fn acknowledge(&self, base: &str, e: &Snapshot) -> Result<()> {
        let tx = self.begin(base).await?;
        let result=async {
            let result=self.query(base,Some(&tx),"SELECT version FROM SpatialIdentity WHERE spatial_id=@id AND tenant_id=@tenant",json!({"id":e.entity_id,"tenant":e.tenant})).await?;
            let version=parse_version(&rows(&result)?.first().ok_or("canonical row missing")?[0])?;
            let mut mutations=vec![mutation("update","Outbox",&["event_id","status"],json!([[e.event_id,"DELIVERED"]]))];
            if version==e.version { mutations.push(mutation("update","SpatialIdentity",&["spatial_id","status"],json!([[e.entity_id,"ACTIVE"]]))); }
            self.commit(base,&tx,mutations).await
        }.await;
        self.rollback(base, &tx).await;
        result
    }
    pub async fn deliver(&self, e: &Snapshot) -> Result<Outcome> {
        e.validate()?;
        let base = self.session().await?;
        let result = async {
            if let Some(done) = self.publish(&base, e).await? {
                return Ok(done);
            }
            let outcome = self.geometry(e).await?;
            self.acknowledge(&base, e).await?;
            Ok(outcome)
        }
        .await;
        let _ = self
            .client
            .delete(&base)
            .bearer_auth(&self.token)
            .send()
            .await;
        result
    }
    pub async fn verify_delivery(&self, e: &Snapshot) -> Result<()> {
        e.validate()?;
        let base = self.session().await?;
        let result=async {
            let v=self.query(&base,None,"SELECT d.details,i.status FROM EventDetails d JOIN SpatialIdentity i ON d.spatial_id=i.spatial_id WHERE d.spatial_id=@id AND d.tenant_id=@tenant AND i.tenant_id=@tenant",json!({"id":e.entity_id,"tenant":e.tenant})).await?;
            let r=rows(&v)?; let row=r.first().ok_or("missing canonical event")?;
            let actual:Snapshot=serde_json::from_str(row[0].as_str().ok_or("invalid canonical JSON")?)?;
            if actual!=*e || row[1]!="ACTIVE" {return Err("canonical event mismatch".into());}
            let graph=self.query(&base,None,"GRAPH SpatialGraph MATCH (e:Entity)-[r:Relationship]->(p:Entity) WHERE e.spatial_id=@id AND e.tenant_id=@tenant AND r.tenant_id=@tenant AND p.tenant_id=@tenant AND r.relation='LOCATED_AT' RETURN p.spatial_id AS place_id",json!({"id":e.entity_id,"tenant":e.tenant})).await?;
            if rows(&graph)?!=vec![vec![json!(linked_id("place",&e.tenant,&e.place))]] {return Err("Graph location mismatch".into());}
            let outbox=self.query(&base,None,"SELECT status FROM Outbox WHERE event_id=@id AND tenant_id=@tenant",json!({"id":e.event_id,"tenant":e.tenant})).await?;
            if rows(&outbox)?!=vec![vec![json!("DELIVERED")]] {return Err("Outbox not acknowledged".into());}
            // Re-delivery reaches the SQL receipt path, independently checking duplicate protection.
            if self.geometry(e).await?!=Outcome::Delivered {return Err("unexpected superseded receipt".into());}
            Ok(())
        }.await;
        let _ = self
            .client
            .delete(base)
            .bearer_auth(&self.token)
            .send()
            .await;
        result
    }
}
fn rows(v: &Value) -> Result<Vec<Vec<Value>>> {
    match v.get("rows") {
        None => Ok(vec![]),
        Some(Value::Array(r)) => r
            .iter()
            .map(|r| r.as_array().cloned().ok_or_else(|| "invalid row".into()))
            .collect(),
        _ => Err("invalid rows".into()),
    }
}
fn parse_version(v: &Value) -> Result<i64> {
    Ok(v.as_str().ok_or("version is not INT64")?.parse()?)
}
fn mutation(op: &str, table: &str, columns: &[&str], values: Value) -> Value {
    let mut v = json!({});
    v[op] = json!({"table":table,"columns":columns,"values":values});
    v
}
fn identity(
    op: &str,
    id: &str,
    kind: &str,
    e: &Snapshot,
    version: i64,
    status: &str,
    created: bool,
) -> Value {
    let mut cols = vec![
        "spatial_id",
        "entity_type",
        "tenant_id",
        "owner_spatial_id",
        "status",
        "trust_level",
        "visibility",
        "version",
    ];
    let mut row = vec![
        json!(id),
        json!(kind),
        json!(e.tenant),
        json!(linked_id("creator", &e.tenant, &e.owner)),
        json!(status),
        json!("1"),
        json!("PRIVATE"),
        json!(version.to_string()),
    ];
    if created {
        cols.push("created_at");
        row.push(json!("spanner.commit_timestamp()"));
    }
    mutation(op, "SpatialIdentity", &cols, json!([row]))
}
fn linked_id(kind: &str, tenant: &str, value: &str) -> String {
    format!(
        "spid_{}",
        &format!("{:x}", Sha256::digest(format!("{kind}\0{tenant}\0{value}")))[..48]
    )
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn sql_result(v: Value) -> Result<Value> {
    if v.get("error").is_some() || v["result"]["isError"] == true {
        return Err("SQL transport error".into());
    }
    let data = v["result"]
        .get("structuredContent")
        .cloned()
        .or_else(|| {
            v["result"]["content"]
                .as_array()?
                .iter()
                .find_map(|x| serde_json::from_str::<Value>(x["text"].as_str()?).ok())
        })
        .ok_or("no SQL result")?;
    if data["metadata"]["status"] != "OK" || data["metadata"]["partialResult"] == true {
        return Err(format!("SQL execution failed: {}", data["metadata"]).into());
    }
    Ok(data)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sql_data_cannot_escape_literal() {
        let h = hex("'); DROP TABLE x; -- 日本語".as_bytes());
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }
    #[test]
    fn sql_partial_or_failed_result_rejected() {
        for m in [
            json!({"status":"ERROR"}),
            json!({"status":"OK","partialResult":true}),
            json!({}),
        ] {
            assert!(sql_result(json!({"result":{"structuredContent":{"metadata":m}}})).is_err());
        }
    }
    #[test]
    fn linked_ids_are_tenant_bound() {
        assert_ne!(
            linked_id("place", "a", "Tokyo"),
            linked_id("place", "b", "Tokyo")
        );
    }
}
