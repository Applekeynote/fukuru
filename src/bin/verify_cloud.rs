//! Explicit, fixed-data integration acceptance; never exposed as an agent tool.
use reqwest::Client;
use serde_json::{json, Value};
use std::{error::Error, time::Duration};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PROJECT: &str = "spatial-atlas-dev-260908-rn";
struct Cloud {
    client: Client,
    token: String,
}
fn sql_result(result: Value) -> Result<Value> {
    if result.get("error").is_some() || result["result"]["isError"] == true {
        return Err(format!("SQL failed: {result}").into());
    }
    let data = result["result"]
        .get("structuredContent")
        .cloned()
        .or_else(|| {
            result["result"]["content"]
                .as_array()?
                .iter()
                .find_map(|c| serde_json::from_str::<Value>(c["text"].as_str()?).ok())
        })
        .ok_or("SQL response has no structured result")?;
    if data["metadata"]["status"] != "OK" || data["metadata"]["partialResult"] == true {
        return Err(format!("SQL execution did not succeed: {data}").into());
    }
    Ok(data)
}
fn proof_valid(proof: &Value) -> bool {
    let row = &proof["sqlResults"][0]["rows"][0]["values"];
    row[0]["value"]
        .as_str()
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        && row[1]["value"] == "1"
        && row[2]["value"] == "1"
}
impl Cloud {
    async fn post(&self, url: &str, body: Value) -> Result<Value> {
        let r = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json, text/event-stream")
            .header("x-goog-user-project", PROJECT)
            .json(&body)
            .send()
            .await?;
        let status = r.status();
        let text = r.text().await?;
        if !status.is_success() {
            return Err(format!(
                "Cloud HTTP {}: {}",
                status,
                text.chars().take(1500).collect::<String>()
            )
            .into());
        }
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
        Err("Cloud returned an unsupported response".into())
    }
    async fn sql(&self, sql: &str) -> Result<Value> {
        let result = self.post("https://alloydb.googleapis.com/mcp", json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"execute_sql","arguments":{"instance":format!("projects/{PROJECT}/locations/asia-northeast1/clusters/atlas-geo-validation/instances/primary"),"database":"postgres","sqlStatement":sql,"validateOnly":false}}})).await?;
        sql_result(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_success_is_not_sql_success() {
        assert!(sql_result(
            json!({"result":{"structuredContent":{"metadata":{"status":"ERROR"}}}})
        )
        .is_err());
        assert!(sql_result(json!({"result":{"structuredContent":{"metadata":{"status":"OK","partialResult":true}}}})).is_err());
        assert!(
            sql_result(json!({"result":{"structuredContent":{"metadata":{"status":"OK"}}}}))
                .is_ok()
        );
    }
    #[test]
    fn zero_radius_matches_cannot_pass() {
        let mut v = json!({"sqlResults":[{"rows":[{"values":[{"value":"3.6"},{"value":"1"},{"value":"0"}]}]}]});
        assert!(!proof_valid(&v));
        v["sqlResults"][0]["rows"][0]["values"][2]["value"] = json!("1");
        assert!(proof_valid(&v));
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    if std::env::var("ATLAS_VERIFY_PROJECT").as_deref() != Ok(PROJECT) {
        return Err("Set ATLAS_VERIFY_PROJECT to the explicit validation project".into());
    }
    let cloud = Cloud {
        client: Client::builder()
            .timeout(Duration::from_secs(90))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        token: std::env::var("ATLAS_ACCESS_TOKEN")?,
    };
    let db = format!("https://spanner.googleapis.com/v1/projects/{PROJECT}/instances/atlas-validation/databases/spatial");
    let session = cloud.post(&format!("{db}/sessions"), json!({})).await?["name"]
        .as_str()
        .ok_or("Missing Spanner session")?
        .to_owned();
    let base = format!("https://spanner.googleapis.com/v1/{session}");
    let run: Result<()> = async {
        // A canonical identity, its relationship and its outbox event commit atomically.
        let ids = json!({"insertOrUpdate":{"table":"SpatialIdentity","columns":["spatial_id","entity_type","tenant_id","owner_spatial_id","status","trust_level","visibility","version","created_at"],"values":[["spid_validation_place","Place","validation","spid_validation_place","ACTIVE","1","PRIVATE","1","spanner.commit_timestamp()"],["spid_validation_event","Event","validation","spid_validation_place","PENDING_GEO","1","PRIVATE","2","spanner.commit_timestamp()"]]}});
        let edge = json!({"insertOrUpdate":{"table":"SpatialRelationship","columns":["source_id","target_id","relation","tenant_id"],"values":[["spid_validation_event","spid_validation_place","LOCATED_AT","validation"]]}});
        let outbox = json!({"insertOrUpdate":{"table":"Outbox","columns":["event_id","entity_id","tenant_id","event_type","schema_version","entity_version","idempotency_key","trace_id","payload","status","occurred_at"],"values":[["evt_validation_geo_v2","spid_validation_event","validation","geometry.upsert","1","2","validation:geometry:2","trace_validation_2","{\"lat\":35.6984,\"lon\":139.7731}","PENDING","spanner.commit_timestamp()"]]}});
        cloud.post(&format!("{base}:commit"),json!({"singleUseTransaction":{"readWrite":{}},"mutations":[ids,edge,outbox]})).await?;
        let graph = cloud.post(&format!("{base}:executeSql"),json!({"sql":"GRAPH SpatialGraph MATCH (e:Entity)-[r:Relationship]->(p:Entity) WHERE e.spatial_id = @id AND e.tenant_id = @tenant AND p.tenant_id = @tenant AND r.tenant_id = @tenant RETURN e.spatial_id AS event_id, p.spatial_id AS place_id, r.relation AS relation","params":{"id":"spid_validation_event","tenant":"validation"},"paramTypes":{"id":{"code":"STRING"},"tenant":{"code":"STRING"}}})).await?;
        if graph["rows"][0][1] != "spid_validation_place" { return Err("Graph relation assertion failed".into()); }
        println!("PASS Spanner canonical transaction + Graph traversal");
        let event=cloud.post(&format!("{base}:executeSql"),json!({"sql":"SELECT event_id, entity_id, tenant_id, entity_version, payload FROM Outbox WHERE event_id=@id","params":{"id":"evt_validation_geo_v2"},"paramTypes":{"id":{"code":"STRING"}}})).await?;
        if event["rows"][0][0] != "evt_validation_geo_v2" {return Err("Outbox event missing".into());}
        // Fixed sample envelope validation precedes the parameter-free SQL fixture.
        let payload:Value=serde_json::from_str(event["rows"][0][4].as_str().ok_or("Outbox payload missing")?)?;
        if payload["lat"]!=35.6984 || payload["lon"]!=139.7731 || event["rows"][0][3]!="2" {return Err("Unexpected sample outbox payload".into());}
        if cloud.sql("DO $$ BEGIN RAISE EXCEPTION 'expected validation rejection'; END $$;").await.is_ok() { return Err("SQL failure was not propagated".into()); }
        println!("PASS SQL failure propagation");
        cloud.sql("CREATE EXTENSION IF NOT EXISTS postgis;").await?;
        cloud.sql(r#"DO $$ BEGIN
          CREATE TABLE IF NOT EXISTS validation_geometry(spatial_id text PRIMARY KEY,tenant_id text NOT NULL,geom geometry(Point,4326) NOT NULL,version bigint NOT NULL CHECK(version>0));
          CREATE INDEX IF NOT EXISTS validation_geometry_gist ON validation_geometry USING gist(geom);
          CREATE TABLE IF NOT EXISTS validation_processed(event_id text PRIMARY KEY);
          ALTER TABLE validation_geometry ENABLE ROW LEVEL SECURITY;
          ALTER TABLE validation_geometry FORCE ROW LEVEL SECURITY;
          IF NOT EXISTS(SELECT 1 FROM pg_policies WHERE tablename='validation_geometry' AND policyname='validation_tenant') THEN
            CREATE POLICY validation_tenant ON validation_geometry USING(tenant_id=current_setting('app.tenant_id',true)) WITH CHECK(tenant_id=current_setting('app.tenant_id',true));
          END IF;
          IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='atlas_validation_reader') THEN CREATE ROLE atlas_validation_reader NOLOGIN NOSUPERUSER NOBYPASSRLS; END IF;
          GRANT SELECT ON validation_geometry TO atlas_validation_reader;
          GRANT atlas_validation_reader TO CURRENT_USER;
        END $$;"#).await?;
        // A single DO statement provides one real PostgreSQL transaction and SET LOCAL scope.
        let deliver=r#"DO $$ BEGIN
          PERFORM set_config('app.tenant_id','validation',true);
          WITH accepted AS (INSERT INTO validation_processed(event_id) VALUES('evt_validation_geo_v2') ON CONFLICT DO NOTHING RETURNING event_id)
          INSERT INTO validation_geometry SELECT 'spid_validation_event','validation',ST_SetSRID(ST_MakePoint(139.7731,35.6984),4326),2 FROM accepted
          ON CONFLICT(spatial_id) DO UPDATE SET geom=excluded.geom,version=excluded.version WHERE validation_geometry.version<excluded.version;
          IF (SELECT count(*) FROM validation_processed WHERE event_id='evt_validation_geo_v2')<>1 THEN RAISE EXCEPTION 'duplicate delivery'; END IF;
          IF NOT EXISTS(SELECT 1 FROM validation_geometry WHERE spatial_id='spid_validation_event' AND version=2 AND ST_DWithin(geom::geography,ST_SetSRID(ST_MakePoint(139.7731,35.6984),4326)::geography,10)) THEN RAISE EXCEPTION 'geometry assertion'; END IF;
        END $$;"#;
        cloud.sql(deliver).await?; cloud.sql(deliver).await?;
        cloud.sql(r#"DO $$ BEGIN
          PERFORM set_config('app.tenant_id','validation',true);
          INSERT INTO validation_geometry VALUES('spid_validation_event','validation',ST_SetSRID(ST_MakePoint(140,36),4326),1)
          ON CONFLICT(spatial_id) DO UPDATE SET geom=excluded.geom,version=excluded.version WHERE validation_geometry.version<excluded.version;
          IF NOT EXISTS(SELECT 1 FROM validation_geometry WHERE spatial_id='spid_validation_event' AND version=2 AND ST_DWithin(geom::geography,ST_SetSRID(ST_MakePoint(139.7731,35.6984),4326)::geography,10)) THEN RAISE EXCEPTION 'stale delivery changed geometry'; END IF;
        END $$;"#).await?;
        cloud.sql(r#"DO $$ BEGIN
          SET LOCAL ROLE atlas_validation_reader;
          PERFORM set_config('app.tenant_id','other-tenant',true);
          IF EXISTS(SELECT 1 FROM validation_geometry) THEN RAISE EXCEPTION 'tenant isolation failed'; END IF;
          PERFORM set_config('app.tenant_id','validation',true);
          IF NOT EXISTS(SELECT 1 FROM validation_geometry WHERE spatial_id='spid_validation_event' AND version=2) THEN RAISE EXCEPTION 'tenant visibility failed'; END IF;
        END $$;"#).await?;
        cloud.sql(r#"CREATE OR REPLACE FUNCTION atlas_validation_proof() RETURNS TABLE(postgis_version text,deliveries bigint,radius_matches bigint)
          LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public AS $$ BEGIN
          PERFORM set_config('app.tenant_id','validation',true);
          RETURN QUERY SELECT public.PostGIS_Version(),(SELECT count(*) FROM public.validation_processed WHERE event_id='evt_validation_geo_v2'),
          (SELECT count(*) FROM public.validation_geometry WHERE spatial_id='spid_validation_event' AND version=2 AND public.ST_DWithin(geom::public.geography,public.ST_SetSRID(public.ST_MakePoint(139.7731,35.6984),4326)::public.geography,10));
        END $$;"#).await?;
        let proof=cloud.sql("SELECT * FROM atlas_validation_proof();").await?;
        println!("PostGIS proof: {proof}");
        if !proof_valid(&proof) { return Err("PostGIS counts failed independent Rust assertions".into()); }
        println!("PASS AlloyDB PostGIS radius + duplicate and stale delivery + RLS tenant isolation");
        cloud.post(&format!("{base}:commit"),json!({"singleUseTransaction":{"readWrite":{}},"mutations":[{"update":{"table":"Outbox","columns":["event_id","status"],"values":[["evt_validation_geo_v2","DELIVERED"]]}},{"update":{"table":"SpatialIdentity","columns":["spatial_id","status"],"values":[["spid_validation_event","ACTIVE"]]}}]})).await?;
        println!("PASS Outbox acknowledgment after durable geometry commit");
        Ok(())
    }.await;
    let _ = cloud
        .client
        .delete(base)
        .bearer_auth(&cloud.token)
        .send()
        .await;
    run
}
