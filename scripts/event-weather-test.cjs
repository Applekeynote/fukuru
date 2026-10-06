const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {JSDOM}=require('../target/navigation-tools/node_modules/jsdom');
const source=fs.readFileSync(path.join(__dirname,'../assets/community.js'),'utf8');
const dom=new JSDOM('<div id="cx-weather"></div>',{url:'http://localhost/',runScripts:'outside-only'}),w=dom.window;
const functions=source.slice(source.indexOf('async function loadWeather('),source.indexOf('async function loadThread('));
w.eval(`let epoch=1,state={me:{id:'a'},ai_available:true},demo=false,allowed=true;
const $=s=>document.querySelector(s),esc=s=>String(s??'').replaceAll('<','&lt;'),fmt=x=>new Date(x).toISOString(),event=()=>({demo}),completion={aiAllowed:()=>allowed};
let handler;const request=(...args)=>handler(...args);
${functions}
window.test={loadWeather,suggestWeather,setHandler:h=>handler=h,leave:()=>epoch++,demo:v=>demo=v,allowed:v=>allowed=v};`);
const t=w.test,$=s=>w.document.querySelector(s),forecast={available:true,forecast_at:1791302400,fetched_at:1791216000,temperature_c:22.5,rain_probability_pct:40,wind_kmh:12,advice:['羽織れる上着'],indoor:false};
(async()=>{
 t.setHandler(async()=>forecast);await t.loadWeather('e');assert.equal($('.rf-weather-metrics dd').textContent,'22.5℃');
 assert(!w.document.body.textContent.includes('global処理'));assert($('[data-weather-ai]'));
 let resolve;t.setHandler((url,body)=>{assert.equal(url,'/api/weather-advice');assert.equal(body.id,'e');return new Promise(r=>resolve=r);});
 const b=$('[data-weather-ai]'),pending=t.suggestWeather(b);assert(b.disabled);assert.equal(b.getAttribute('aria-busy'),'true');
 resolve({advice:'薄手の上着と折り畳み傘を。'});await pending;assert.equal($('#cx-weather-ai').textContent,'薄手の上着と折り畳み傘を。');assert(!b.disabled);
 t.setHandler(async()=>{throw Error('天気予報を取得できません');});await t.suggestWeather(b);assert($('#cx-weather-ai').classList.contains('rf-weather-error'));assert(b.textContent.includes('再試行'));assert(!b.disabled);
 t.setHandler(()=>new Promise(r=>resolve=r));const leaving=t.suggestWeather(b);t.leave();$('#cx-weather-ai').textContent='別のイベント';resolve({advice:'旧イベントの返答'});await leaving;assert.equal($('#cx-weather-ai').textContent,'別のイベント');
 t.setHandler(async()=>forecast);t.demo(true);await t.loadWeather('demo');assert(!$('[data-weather-ai]'),'demo advice is not offered when the server rejects it');
 t.demo(false);t.allowed(false);await t.loadWeather('e');assert(!$('[data-weather-ai]'),'AI preference is respected');
 t.allowed(true);t.setHandler(async()=>({available:false,reason:'開催日時の予報がありません'}));await t.loadWeather('e');assert.equal($('#cx-weather').textContent,'開催日時の予報がありません');assert(!$('.rf-weather-metrics'));
 console.log('PASS: forecast metrics, consent copy removal, AI loading/success/retry, stale response isolation, demo/preference gating, unavailable forecast');
 dom.window.close();
})().catch(e=>{console.error(e);process.exitCode=1;});
