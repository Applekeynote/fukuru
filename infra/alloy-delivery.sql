DO $migration$ BEGIN
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE TABLE IF NOT EXISTS atlas_geometry (
 tenant_id text NOT NULL, spatial_id text NOT NULL,
 geom geometry(Point,4326) NOT NULL, version bigint NOT NULL CHECK(version>0),
 payload_hash text NOT NULL, PRIMARY KEY(tenant_id,spatial_id)
);
CREATE INDEX IF NOT EXISTS atlas_geometry_radius ON atlas_geometry USING gist ((geom::geography));
CREATE TABLE IF NOT EXISTS atlas_processed (
 tenant_id text NOT NULL, event_id text NOT NULL, spatial_id text NOT NULL,
 payload_hash text NOT NULL, outcome text NOT NULL,
 entity_version bigint NOT NULL, PRIMARY KEY(tenant_id,event_id)
);
ALTER TABLE atlas_geometry ENABLE ROW LEVEL SECURITY;
ALTER TABLE atlas_geometry FORCE ROW LEVEL SECURITY;
ALTER TABLE atlas_processed ENABLE ROW LEVEL SECURITY;
ALTER TABLE atlas_processed FORCE ROW LEVEL SECURITY;
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM pg_policies WHERE schemaname='public' AND tablename='atlas_geometry' AND policyname='atlas_tenant') THEN
  CREATE POLICY atlas_tenant ON atlas_geometry USING(tenant_id=current_setting('app.tenant_id',true)) WITH CHECK(tenant_id=current_setting('app.tenant_id',true));
 END IF;
 IF NOT EXISTS(SELECT 1 FROM pg_policies WHERE schemaname='public' AND tablename='atlas_processed' AND policyname='atlas_tenant') THEN
  CREATE POLICY atlas_tenant ON atlas_processed USING(tenant_id=current_setting('app.tenant_id',true)) WITH CHECK(tenant_id=current_setting('app.tenant_id',true));
 END IF;
END $$;
-- One statement is one transaction; session context never depends on the next API call.
CREATE OR REPLACE FUNCTION atlas_apply_delivery(p jsonb, h text) RETURNS jsonb
 LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,public AS $$
DECLARE t text:=p->>'tenant'; eid text:=p->>'event_id'; sid text:=p->>'entity_id';
 v bigint:=(p->>'version')::bigint; lat double precision:=(p->>'lat')::double precision;
 lon double precision:=(p->>'lon')::double precision; old atlas_geometry%ROWTYPE;
 receipt atlas_processed%ROWTYPE; result text:='APPLIED';
BEGIN
 IF t IS DISTINCT FROM 'studio' OR (p->>'owner') IS DISTINCT FROM 'creator-local' OR (p->>'schema_version') IS DISTINCT FROM '1'
  OR eid IS NULL OR sid IS NULL OR h IS NULL OR length(h)<>64 OR v IS NULL OR v<1 OR lat IS NULL OR lon IS NULL
  OR NOT (lat BETWEEN -90 AND 90 AND lon BETWEEN -180 AND 180) THEN RAISE EXCEPTION 'invalid delivery'; END IF;
 PERFORM set_config('app.tenant_id',t,true);
 PERFORM pg_advisory_xact_lock(hashtextextended(t||':'||sid,0));
 SELECT * INTO receipt FROM atlas_processed WHERE tenant_id=t AND event_id=eid;
 IF FOUND THEN
  IF receipt.payload_hash<>h OR receipt.spatial_id<>sid OR receipt.entity_version<>v THEN RAISE EXCEPTION 'idempotency conflict'; END IF;
  result:=receipt.outcome;
 ELSE
  SELECT * INTO old FROM atlas_geometry WHERE tenant_id=t AND spatial_id=sid FOR UPDATE;
  IF FOUND AND old.version=v AND old.payload_hash<>h THEN RAISE EXCEPTION 'version conflict'; END IF;
  IF FOUND AND old.version>v THEN result:='SUPERSEDED';
  ELSE
   INSERT INTO atlas_geometry VALUES(t,sid,ST_SetSRID(ST_MakePoint(lon,lat),4326),v,h)
   ON CONFLICT(tenant_id,spatial_id) DO UPDATE SET geom=excluded.geom,version=excluded.version,payload_hash=excluded.payload_hash WHERE atlas_geometry.version<excluded.version;
  END IF;
  INSERT INTO atlas_processed VALUES(t,eid,sid,h,result,v);
 END IF;
 RETURN jsonb_build_object('event_id',eid,'outcome',result,'version',v,'receipt_count',
  (SELECT count(*) FROM atlas_processed WHERE tenant_id=t AND event_id=eid),
  'radius_match',EXISTS(SELECT 1 FROM atlas_geometry WHERE tenant_id=t AND spatial_id=sid AND version>=v
    AND (version>v OR ST_DWithin(geom::geography,ST_SetSRID(ST_MakePoint(lon,lat),4326)::geography,0.1))));
END $$;
REVOKE ALL ON FUNCTION atlas_apply_delivery(jsonb,text) FROM PUBLIC;
END $migration$;
