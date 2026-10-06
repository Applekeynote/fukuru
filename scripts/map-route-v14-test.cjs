'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs');
const {JSDOM}=require('../target/navigation-tools/node_modules/jsdom');
const source=fs.readFileSync('assets/community.js','utf8');
const map=source.slice(source.indexOf('function mapPage(){'),source.indexOf('function render(){'));
const render=source.slice(source.indexOf('function render(){'),source.indexOf('let resultPage='));
const setup=source.slice(source.indexOf('function setupMap(){'),source.indexOf('function setupMap(){')+source.slice(source.indexOf('function setupMap(){')).indexOf('\n'));
assert(map.startsWith('function mapPage(){'));
for(const view of ['map','glasses']){
 const dom=new JSDOM('<input id="cx-search"><main id="cx-main"></main>',{runScripts:'outside-only',url:'https://example.test/?view='+view+'&id=owned'}),w=dom.window;
 w.eval(fs.readFileSync('assets/spatial.js','utf8'));
 w.mounts=[];w.SpatialExplore.mount=options=>{w.mounts.push(options);return {stop(){}}};
 w.eval(`let state={events:[],me:{id:'owner'}},epoch=0,threadTimer=null,mapController=null,view='',selected='',recommendMode='all',searchTab='events';
 const $=s=>document.querySelector(s),esc=String,go=()=>{},comment=()=>{},proposeActions=()=>{},ui={shell(){}},nav=[['map',null,'地図']],completion={enhance(){}},refine={enhance(){}},recordHistory=()=>{},updateSuggestions=()=>{},proactive=()=>{},refreshEventStates=()=>{};
 ${map}${setup}${render}
 render();`);
 assert(w.document.querySelector('#sx-map'));
 assert(w.document.querySelector('#sx-planned'));
 assert(w.document.querySelector('#sx-events'));
 assert.equal(w.mounts.length,1);
 assert.equal(w.mounts[0].selected,'owned');
 assert.equal(w.mounts[0].getState().me.id,'owner');
 dom.window.close();
}
console.log('PASS actual map/glasses route render and mount; planned/nearby lists; selected destination and account state retained');
