//! Private, bounded logical snapshots for the free Spanner instance.
use spatial_atlas::community::{store::{Store,Records,access_token},model};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
fn digest(v:&Value)->String{format!("{:x}",Sha256::digest(v.to_string().as_bytes()))}
#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>>{
 let args:Vec<_>=std::env::args().collect();
 if args.get(1).map(String::as_str)==Some("verify-restore"){
  let file=args.get(2).ok_or("snapshot file required")?;let destination=args.get(3).ok_or("new local database required")?;
  if std::path::Path::new(destination).exists(){return Err("restore destination must not exist".into());}
  let doc:Value=serde_json::from_slice(&std::fs::read(file)?)?;
  if doc["format"]!="spatial-snapshot-v1"||doc["sha256"]!=digest(&doc["records"]){return Err("snapshot integrity failed".into());}
  let mut rows=Records::new();for row in doc["records"].as_array().ok_or("records missing")?{let a=row.as_array().ok_or("invalid row")?;if a.len()!=3{return Err("invalid row length".into());}let key=(a[0].as_str().ok_or("invalid scope")?.into(),a[1].as_str().ok_or("invalid key")?.into());if rows.insert(key,a[2].clone()).is_some(){return Err("duplicate row".into());}}
  let count=rows.len();let local=Store::local(destination)?;local.transact(|r|{*r=rows.clone();Ok(())}).await?;
  local.transact(|r|{for (key,v) in &rows{if r.get(key)!=Some(v){return Err("restored record mismatch".into());}}Ok(())}).await?;
  println!("{}",json!({"restored_records":count,"integrity":"verified","source_at":doc["at"]}));return Ok(());
 }
 let store=if let Ok(path)=std::env::var("ATLAS_COMMUNITY_DB"){Store::local(&path)?}else{Store::cloud()?};
 let records=store.transact(|r|Ok(Value::Array(r.iter().map(|((s,k),v)|json!([s,k,v])).collect()))).await?;
 let at=model::now();let count=records.as_array().unwrap().len();let sha=digest(&records);let snapshot=json!({"format":"spatial-snapshot-v1","at":at,"sha256":sha,"records":records});
 if let Some(file)=args.get(1){std::fs::write(file,snapshot.to_string())?;}else{
  let bucket=std::env::var("SPATIAL_BACKUP_BUCKET")?;
  if bucket!="spatial-atlas-dev-260908-rn-community-backups"{return Err("unexpected backup destination".into());}
  let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(90)).build()?;let token=access_token(&client).await?;
  let name=format!("community/{at}-{}.json",uuid::Uuid::new_v4());
  let response=client.post(format!("https://storage.googleapis.com/upload/storage/v1/b/{bucket}/o")).bearer_auth(token).query(&[("uploadType","media"),("name",name.as_str()),("ifGenerationMatch","0")]).header("Content-Type","application/json").body(snapshot.to_string()).send().await?;
  if !response.status().is_success(){return Err(format!("backup upload failed: {}",response.status()).into());}
 }
 println!("{}",json!({"backup_at":at,"records":count,"sha256":sha,"status":"saved"}));Ok(())
}
