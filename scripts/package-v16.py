from pathlib import Path
import tarfile, hashlib

root=Path(__file__).resolve().parents[1]
output=root/'releases/spatial-community-20261007-source-v16.tar.gz'
files=[root/'Cargo.toml',root/'Cargo.lock']
for folder in ['assets','src']:
    files.extend(p for p in (root/folder).rglob('*') if p.is_file())
for name in [
 'infra/alloy-delivery.sql','infra/community-geo.sql','infra/backup-lifecycle.json','infra/Dockerfile.community','infra/Dockerfile.backup',
 'scripts/event-state-test.cjs','scripts/community-test.cjs','scripts/completion-test.cjs','scripts/location-test.cjs','scripts/spatial-test.cjs',
 'scripts/refinement-test.cjs','scripts/routes-test.cjs','scripts/identity-images-test.cjs','scripts/build-ar-ui-harness.py',
 'scripts/navigation-test.cjs','scripts/navigation-dom-test.cjs','scripts/navigation-api-test.cjs','scripts/navigation-browser-harness.cjs',
 'scripts/navigation-eslint.config.mjs','scripts/navigation-tools.package.json','scripts/inbox-test.cjs','scripts/inbox-api-test.cjs',
 'scripts/inbox-browser-qa.cjs','scripts/event-weather-test.cjs','scripts/participation-v13-test.cjs','scripts/join-ui-v13-test.cjs',
 'scripts/generate-regional-events.py','scripts/package-v16.py','scripts/calendar-v16-test.cjs','docs/acceptance-20261007-v16.md','scripts/map-route-v14-test.cjs','scripts/discovery-v14-test.cjs','data/regional-events-2026-q4.json',
 'data/regional-profiles-v15.json','scripts/calendar-v15-test.cjs','scripts/planner-v15-test.cjs','scripts/lifecycle-v15-test.cjs','docs/acceptance-20261007-v15.md','docs/navigation-architecture-v10.md','docs/acceptance-20261005-v12.md','docs/acceptance-20261006-v13.md','docs/acceptance-20261006-v14.md','docs/acceptance-20261006-v14a.md',
]:files.append(root/name)
assert not output.exists(), 'Immutable release already exists'
assert all(p.is_file() for p in files)
assert not any(p.suffix.lower() in ['.db','.log','.sqlite','.env'] for p in files)
with tarfile.open(output,'w:gz') as archive:
 for path in sorted(set(files)):archive.add(path,arcname=path.relative_to(root).as_posix(),recursive=False)
with tarfile.open(output) as archive:
 names=archive.getnames()
 assert 'src/bin/spatial_demo_import.rs' in names and 'data/regional-events-2026-q4.json' in names
 assert all(not n.startswith(('target/','releases/','.git/')) for n in names)
print(output)
print(f'{output.stat().st_size} bytes; {len(names)} files')
print('SHA-256: '+hashlib.sha256(output.read_bytes()).hexdigest().upper())
