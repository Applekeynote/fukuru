// Local acceptance harness. Never imported by the product, never served on public origins.
const http=require('node:http'),fs=require('node:fs'),path=require('node:path');const assets=path.join(__dirname,'../assets');
const setup=`
const fixtureClock={at:Date.now()};Date.now=()=>fixtureClock.at;let onGPS;
Object.defineProperty(navigator,'userAgent',{value:'iPhone local acceptance fixture'});Object.defineProperty(navigator,'maxTouchPoints',{value:5});
Object.defineProperty(navigator,'geolocation',{value:{watchPosition(ok){onGPS=ok;return 1;},clearWatch(){}}});
Object.defineProperty(navigator,'mediaDevices',{value:{getUserMedia:async()=>{const c=document.createElement('canvas');c.width=480;c.height=640;const x=c.getContext('2d');x.fillStyle='#8c9497';x.fillRect(0,0,480,640);x.fillStyle='#657475';x.fillRect(0,360,480,280);return c.captureStream(1);}}});
google={maps:{Map:class{}}};
const origin={lat:35,lon:139},turn={lat:35.001,lon:139},end={lat:35.001,lon:139.001};let hazard=false;
SpatialRoutes.route=async(from,to,mode)=>({path:[origin,turn,end],steps:[{start:turn,maneuver:'TURN_RIGHT',text:hazard?'横断歩道':'右折'}],mode,durationMillis:240000,distanceMeters:200,warnings:[]});
const event={id:'fixture',name:'森の写真さんぽ',place:'公園の入口',lat:end.lat,lon:end.lon,start:new Date(fixtureClock.at+60000).toISOString(),end:new Date(fixtureClock.at+86400000).toISOString(),kind:'WALK'};
const state={events:[event],following:[],google_maps_key:'',me:null};document.querySelector('main').innerHTML=SpatialExplore.page();
const controller=SpatialExplore.mount({getState:()=>state,esc:x=>String(x??''),go(){},comment(){},propose(){},selected:'fixture'});
const sample=(p,speed=0,accuracy=5)=>{fixtureClock.at+=11000;onGPS({coords:{latitude:p.lat,longitude:p.lon,accuracy,speed,heading:0},timestamp:fixtureClock.at});const e=new Event('deviceorientation');Object.assign(e,{absolute:true,alpha:0});window.dispatchEvent(e);};
document.querySelector('[data-test=walk]').onclick=()=>sample(origin,1);
document.querySelector('[data-test=stop]').onclick=()=>{sample(origin,0);sample(origin,0);};
document.querySelector('[data-test=turn]').onclick=()=>{fixtureClock.at+=50000;sample({lat:35.0008,lon:139},1);};
document.querySelector('[data-test=cycle]').onclick=()=>sample(origin,6);
document.querySelector('[data-test=drive]').onclick=()=>sample(origin,15);
document.querySelector('[data-test=low]').onclick=()=>sample(origin,1,100);
document.querySelector('[data-test=arrival]').onclick=()=>{fixtureClock.at+=50000;sample(end,0);sample(end,0);sample(end,0);sample(end,0);};
sample(origin);document.querySelector('#sx-events [data-guide]').click();
`;
const scripts=['location.js','event-state.js','routes.js','navigation.js','navigation-rewards.js','navigation-renderer.js','spatial.js'];
const css=['style.css','community.css','spatial.css','product.css','completion.css','experience.css','refinement.css','navigation.css','inbox.css'];
const html=`<!doctype html><html lang="ja"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Spatial local navigation acceptance</title>${css.map(f=>'<link rel="stylesheet" href="/'+f+'">').join('')}<style>body{margin:0}main{max-width:1100px;padding:24px;margin:auto}.fixture-controls{display:flex;gap:4px;padding:8px;background:#f4f5f6;border-bottom:1px solid #ddd}.fixture-controls button{min-width:44px;min-height:44px;font-size:13px}#poyo-root{display:none!important}@media(max-width:600px){main{padding:16px}}</style><body class="cx-body"><div class="fixture-controls" aria-label="ローカル検証用センサー入力">${[['walk','↑'],['stop','Ⅱ'],['turn','↱'],['cycle','○'],['drive','▣'],['low','—'],['arrival','✓']].map(([id,label])=>'<button data-test="'+id+'" aria-label="'+id+' evidence">'+label+'</button>').join('')}</div><main></main><div id="poyo-root"><button id="poyo-toggle"></button><section id="cx-poyo-panel" hidden></section></div>${scripts.map(f=>'<script src="/'+f+'"></script>').join('')}<script>${setup}</script></body></html>`;
http.createServer((req,res)=>{const name=new URL(req.url,'http://127.0.0.1').pathname.slice(1);if(!name){res.setHeader('Content-Type','text/html; charset=utf-8');res.end(html);return;}if(!/^[a-z0-9-]+\.(js|css|svg)$/.test(name)){res.writeHead(404);res.end();return;}const file=path.join(assets,name);if(!fs.existsSync(file)){res.writeHead(404);res.end();return;}res.setHeader('Content-Type',name.endsWith('.js')?'text/javascript':name.endsWith('.css')?'text/css':'image/svg+xml');res.end(fs.readFileSync(file));}).listen(Number(process.env.NAV_QA_PORT||8808),'127.0.0.1',()=>console.log('Local acceptance harness http://127.0.0.1:'+(process.env.NAV_QA_PORT||8808)));
