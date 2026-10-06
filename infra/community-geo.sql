CREATE EXTENSION IF NOT EXISTS postgis;
CREATE TABLE IF NOT EXISTS community_geometry (
 id text PRIMARY KEY, owner_id text NOT NULL, version bigint NOT NULL,
 geom geometry(Point,4326) NOT NULL, payload_hash text NOT NULL
);
CREATE INDEX IF NOT EXISTS community_geometry_radius ON community_geometry USING gist ((geom::geography));
CREATE TABLE IF NOT EXISTS community_geo_receipt (delivery_id text PRIMARY KEY, payload_hash text NOT NULL);
REVOKE ALL ON community_geometry, community_geo_receipt FROM PUBLIC;
CREATE OR REPLACE FUNCTION public.community_apply(e jsonb, delivery text, digest text)
RETURNS text LANGUAGE plpgsql SECURITY INVOKER AS $$
DECLARE prior text; result text;
BEGIN
 IF (e->>'version')::bigint<1 OR abs((e->>'lat')::double precision)>90 OR abs((e->>'lon')::double precision)>180 THEN RAISE EXCEPTION 'invalid event'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended(e->>'id',0));
 SELECT payload_hash INTO prior FROM community_geo_receipt WHERE delivery_id=delivery;
 IF FOUND THEN
  IF prior<>digest THEN RAISE EXCEPTION 'receipt mismatch'; END IF;
  RETURN 'DUPLICATE';
 END IF;
 SELECT payload_hash INTO prior FROM community_geometry WHERE id=e->>'id' AND version=(e->>'version')::bigint;
 IF FOUND AND prior<>digest THEN RAISE EXCEPTION 'version conflict'; END IF;
 INSERT INTO community_geometry VALUES(e->>'id',e->>'owner',(e->>'version')::bigint,ST_SetSRID(ST_MakePoint((e->>'lon')::double precision,(e->>'lat')::double precision),4326),digest)
 ON CONFLICT(id) DO UPDATE SET owner_id=excluded.owner_id, version=excluded.version, geom=excluded.geom, payload_hash=excluded.payload_hash WHERE community_geometry.version<excluded.version;
 result:=CASE WHEN FOUND THEN 'APPLIED' ELSE 'SUPERSEDED' END;
 INSERT INTO community_geo_receipt VALUES(delivery,digest);
 RETURN result;
END $$;
REVOKE ALL ON FUNCTION public.community_apply(jsonb,text,text) FROM PUBLIC;
