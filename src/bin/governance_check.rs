//! Read-only project baseline gate. Missing evidence fails closed.
use serde_json::Value;
use std::{fs, path::Path};
const PROJECT: &str = "spatial-atlas-dev-260908-rn";
fn read(dir: &Path, name: &str) -> Value {
    serde_json::from_str(
        &fs::read_to_string(dir.join(format!("{name}.json"))).expect("missing evidence"),
    )
    .expect("invalid evidence")
}
fn items(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn metadata(v: &Value, key: &str) -> bool {
    items(v).iter().any(|x| {
        x["key"] == key
            && x["value"]
                .as_str()
                .unwrap_or("")
                .eq_ignore_ascii_case(if key == "enable-oslogin" {
                    "TRUE"
                } else {
                    "FALSE"
                })
    })
}
fn enforced(v: &Value, constraint: &str) -> bool {
    v["constraint"] == format!("constraints/{constraint}") && v["booleanPolicy"]["enforced"] == true
}
fn evaluate(dir: &Path, idle: bool) -> Vec<(&'static str, bool)> {
    let project = read(dir, "project");
    let vm = read(dir, "vm");
    let iam = read(dir, "iam");
    let fw = read(dir, "firewalls");
    let alloy = read(dir, "alloy");
    let spanner = read(dir, "spanner");
    let scheduler = read(dir, "scheduler");
    let buckets = read(dir, "buckets");
    let keys = read(dir, "keys");
    let mut checks = vec![
        (
            "project identity",
            // Compute's resource id differs from Resource Manager's project number.
            project["name"] == PROJECT && project["id"].as_str() == Some("9037590231602305048"),
        ),
        (
            "project OS Login required",
            metadata(
                &project["commonInstanceMetadata"]["items"],
                "enable-oslogin",
            ),
        ),
        (
            "project serial console disabled",
            metadata(
                &project["commonInstanceMetadata"]["items"],
                "serial-port-enable",
            ),
        ),
        (
            "VM scoped network",
            vm["name"] == "atlas-dev"
                && items(&vm["networkInterfaces"]).len() == 1
                && vm["networkInterfaces"][0]["network"]
                    .as_str()
                    .unwrap_or("")
                    .ends_with("/networks/atlas-net"),
        ),
        (
            "VM no service account",
            items(&vm["serviceAccounts"]).is_empty(),
        ),
        (
            "VM OS Login",
            metadata(&vm["metadata"]["items"], "enable-oslogin"),
        ),
        (
            "VM serial console disabled",
            metadata(&vm["metadata"]["items"], "serial-port-enable"),
        ),
        (
            "VM secure boot",
            vm["shieldedInstanceConfig"]["enableSecureBoot"] == true,
        ),
        (
            "no public project IAM",
            iam["bindings"].is_array()
                && items(&iam["bindings"]).iter().all(|b| {
                    items(&b["members"])
                        .iter()
                        .all(|m| m != "allUsers" && m != "allAuthenticatedUsers")
                }),
        ),
        (
            "ingress restricted to IAP SSH",
            fw.is_array()
                && items(&fw).iter().all(|f| {
                    f["disabled"] == true
                        || f["direction"] == "EGRESS"
                        || (f["name"] == "atlas-iap-ssh"
                            && f["sourceRanges"] == serde_json::json!(["35.235.240.0/20"])
                            && f["allowed"]
                                == serde_json::json!([{"IPProtocol":"tcp","ports":["22"]}]))
                }),
        ),
        (
            "AlloyDB no public IP",
            alloy["networkConfig"]["enablePublicIp"] != true
                && alloy["publicIpAddress"].as_str().unwrap_or("").is_empty(),
        ),
        (
            "Spanner free tier",
            spanner["instanceType"] == "FREE_INSTANCE",
        ),
        (
            "billing stop enabled",
            scheduler["state"] == "ENABLED"
                && scheduler["schedule"] == "0 0 1 12 *"
                && scheduler["timeZone"] == "Asia/Tokyo"
                && scheduler["httpTarget"]["httpMethod"] == "PUT"
                && scheduler["httpTarget"]["uri"]
                    == format!(
                        "https://cloudbilling.googleapis.com/v1/projects/{PROJECT}/billingInfo"
                    )
                && scheduler["httpTarget"]["oauthToken"]["serviceAccountEmail"]
                    == format!("atlas-billing-stop@{PROJECT}.iam.gserviceaccount.com"),
        ),
        // A newly introduced bucket needs its own PAP/UBLA and IAM evidence before release.
        (
            "no unreviewed storage buckets",
            buckets.is_array() && items(&buckets).is_empty(),
        ),
        (
            "no user managed SA keys",
            keys.is_array() && items(&keys).is_empty(),
        ),
    ];
    for (service, label) in [
        ("spanner.googleapis.com", "Spanner write audit"),
        ("alloydb.googleapis.com", "AlloyDB write audit"),
        ("iam.googleapis.com", "IAM write audit"),
        (
            "cloudresourcemanager.googleapis.com",
            "resource manager write audit",
        ),
    ] {
        checks.push((
            label,
            items(&iam["auditConfigs"]).iter().any(|c| {
                c["service"] == service
                    && items(&c["auditLogConfigs"]).iter().any(|l| {
                        l["logType"] == "DATA_WRITE" && items(&l["exemptedMembers"]).is_empty()
                    })
            }),
        ));
    }
    let hierarchy = read(dir, "hierarchy");
    checks.push((
        "approved organization",
        hierarchy["parent"]["id"] == "613250587997",
    ));
    for (policy, label) in [
        (
            "iam.disableServiceAccountKeyCreation",
            "SA key creation prohibited",
        ),
        (
            "iam.disableServiceAccountKeyUpload",
            "SA key upload prohibited",
        ),
        (
            "iam.automaticIamGrantsForDefaultServiceAccounts",
            "default SA automatic roles prohibited",
        ),
        (
            "storage.publicAccessPrevention",
            "storage public access prohibited",
        ),
        (
            "storage.uniformBucketLevelAccess",
            "uniform storage IAM required",
        ),
        ("compute.requireOsLogin", "OS Login policy enforced"),
        (
            "compute.disableSerialPortAccess",
            "serial console policy enforced",
        ),
    ] {
        checks.push((label, enforced(&read(dir, policy), policy)));
    }
    let run = read(dir, "run");
    checks.push((
        "community runtime identity",
        run["spec"]["template"]["spec"]["serviceAccountName"]
            == format!("spatial-community@{PROJECT}.iam.gserviceaccount.com"),
    ));
    checks.push((
        "one runtime instance maximum",
        run["spec"]["template"]["metadata"]["annotations"]["autoscaling.knative.dev/maxScale"]
            == "1",
    ));
    if idle {
        checks.push(("VM stopped", vm["status"] == "TERMINATED"));
        checks.push((
            "AlloyDB stopped",
            alloy["state"] == "STOPPED" && alloy["activationPolicy"] == "NEVER",
        ));
    }
    checks
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = Path::new(
        args.get(1)
            .expect("usage: governance_check EVIDENCE_DIR [--idle]"),
    );
    let checks = evaluate(dir, args.iter().any(|s| s == "--idle"));
    let mut failed = false;
    for (name, ok) in checks {
        println!("{}: {name}", if ok { "PASS" } else { "FAIL" });
        failed |= !ok;
    }
    if failed {
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_requires_exact_key_and_value() {
        assert!(metadata(
            &serde_json::json!([{"key":"enable-oslogin","value":"TRUE"}]),
            "enable-oslogin"
        ));
        assert!(!metadata(&Value::Null, "enable-oslogin"));
        assert!(!metadata(
            &serde_json::json!([{"key":"serial-port-enable","value":"true"}]),
            "serial-port-enable"
        ));
    }
    #[test]
    fn policy_requires_matching_enforced_constraint() {
        let v = serde_json::json!({"constraint":"constraints/iam.disableServiceAccountKeyCreation","booleanPolicy":{"enforced":true}});
        assert!(enforced(&v, "iam.disableServiceAccountKeyCreation"));
        assert!(!enforced(&v, "iam.disableServiceAccountKeyUpload"));
        assert!(!enforced(
            &serde_json::json!({"constraint":"constraints/iam.disableServiceAccountKeyCreation","booleanPolicy":{"enforced":false}}),
            "iam.disableServiceAccountKeyCreation"
        ));
        assert!(!enforced(
            &Value::Null,
            "iam.disableServiceAccountKeyCreation"
        ));
    }
}
