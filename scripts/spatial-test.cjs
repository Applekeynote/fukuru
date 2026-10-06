'use strict';
const assert=require('node:assert/strict'),s=require('../assets/spatial.js');
const t=Date.parse('2026-09-23T12:00:00Z');
assert.deepEqual(s.active([{id:'past',end:'2026-09-23T11:59:59Z'},{id:'edge',end:'2026-09-23T12:00:00Z'},{id:'future',end:'2026-09-23T12:00:01Z'},{id:'invalid',end:'bad'}],t).map(x=>x.id),['future']);
const events=[{id:'a',x:10,y:10},{id:'b',x:12,y:12},{id:'c',x:150,y:150}];
assert.deepEqual(s.clusters(events,e=>e).map(g=>g.events.length),[2,1]);
assert.deepEqual(s.clusters([...events].reverse(),e=>e).map(g=>g.events.map(e=>e.id)),[['a','b'],['c']]);
assert.equal(s.floor({floor:-2}),'B2');assert.equal(s.floor({floor:3}),'3F');assert.equal(s.floor({floor:null}),'フロア未登録');
const a={lat:35,lon:139,accuracy:15};assert.equal(s.near(a,a),true);assert.equal(s.near({...a,accuracy:101},a),false);assert.equal(s.near(a,{lat:36,lon:139}),false);assert.equal(s.near(null,a),false);
assert.ok(s.distance(a,{lat:35.001,lon:139})>100);assert.equal(s.bearing(a,{lat:36,lon:139}),0);
assert.deepEqual(s.offlineRegion('名古屋市'),{lat:35.1709,lon:136.8815});
assert.deepEqual(s.offlineRegion('東京都'),{lat:35.6812,lon:139.7671});
assert.equal(s.offlineRegion('未知の住所'),null);
console.log('PASS expiration boundary, clustering, floor, distance and proximity accuracy');

const future={start:'2026-10-12T01:00:00Z',end:'2026-10-12T02:00:00Z',extra:{mode:'onsite'}};
const plans=[{...future,id:'mine',owner:'me',lat:43,lon:141},{...future,id:'joined',owner:'other',my_status:'going'},{...future,id:'nearby',owner:'other'},{...future,id:'cancel',owner:'me',status:'canceled'},{...future,id:'past',owner:'me',end:'2020-01-01T00:00:00Z'},{...future,id:'online',owner:'me',extra:{mode:'online'}}];
assert.deepEqual(s.planned(plans,{id:'me'},Date.parse('2026-10-06')).map(e=>e.id),['joined','mine']);assert.deepEqual(s.planned(plans,null),[]);
console.log('PASS own + joined upcoming plans independent of viewport, excludes canceled/ended/online');
