'use strict';
(function(host){
// The scheduled interval is half-open: start <= now < end. Cancellation wins.
function phase(e,now=Date.now()){
 if(!e)return 'unknown';
 if(e.deleted||e.status==='canceled')return 'canceled';
 const start=Date.parse(e.start),end=Date.parse(e.end);
 if(!Number.isFinite(start)||!Number.isFinite(end)||end<=start)return 'unknown';
 return now>=end?'ended':now>=start?'live':'upcoming';
}
function label(e,now){const p=phase(e,now),en=host.SpatialIdentity?.language?.().startsWith('en');return ({live:en?'Happening now':'開催中',upcoming:en?'Upcoming':'開催予定',ended:en?'Ended':'終了',canceled:en?'Canceled':'中止',unknown:en?'Schedule unconfirmed':'日時未確認'})[p];}
function update(node,e,now){const p=phase(e,now),text=label(e,now);node.className='ev-state ev-state--'+p;node.dataset.eventState=e.id;node.setAttribute('aria-label',text);node.title='登録された開催日時に基づく表示';if(node.textContent!==text)node.textContent=text;}
function node(e,now){const n=document.createElement('span');update(n,e,now);return n;}
function decorate(getState,getView,getSelected){
 const state=getState();if(!state)return;const now=Date.now(),find=id=>state.events.find(e=>e.id===id);
 document.querySelectorAll('[data-event-state]').forEach(n=>{const e=find(n.dataset.eventState);if(e)update(n,e,now);});
 document.querySelectorAll('#cx-main a[data-nav]').forEach(a=>{let u;try{u=new URL(a.href,location.href);}catch{return;}if(u.searchParams.get('view')!=='event')return;const e=find(u.searchParams.get('id'));if(!e||!a.textContent.includes(e.name)||a.querySelector('[data-event-state]'))return;const card=a.closest('.cx-card,.cx-brief-card,.sx-plan-row');if(card?.querySelector('[data-event-state]'))return;const title=a.querySelector('h3');if(title){if(a.previousElementSibling?.matches('.ev-title-state'))return;const row=document.createElement('div');row.className='ev-title-state';row.append(node(e,now));a.before(row);}else a.append(node(e,now));});
 if(getView()==='event'){const e=find(getSelected()),header=document.querySelector('.rf-event-heading');if(e&&header&&!header.querySelector('[data-event-state]'))header.prepend(node(e,now));}
 if(getView()==='messages'){const room=state.rooms.find(r=>r.id===getSelected())||state.rooms[0],e=room?.event&&find(room.event),header=document.querySelector('.rf-chat-header h2');if(e&&header&&!header.querySelector('[data-event-state]'))header.append(node(e,now));}
 document.querySelectorAll('a[data-nav]').forEach(a=>{let u;try{u=new URL(a.href,location.href);}catch{return;}const id=u.searchParams.get('id');if(u.searchParams.get('view')!=='messages'||!id?.startsWith('event-'))return;const e=find(id.slice(6)),title=a.querySelector('.rf-room-copy b')||a.closest('.ui-feed-talk')?.querySelector('.ui-feed-meta b');if(e&&title&&!title.querySelector('[data-event-state]'))title.append(node(e,now));});
 document.querySelectorAll('#cx-dialog input[name^="event:"]').forEach(input=>{const e=find(input.name.slice(6)),label=input.closest('label');if(e&&label&&!label.querySelector('[data-event-state]'))label.append(node(e,now));});
 document.querySelectorAll('#cx-dialog h3').forEach(h=>{if(h.querySelector('[data-event-state]'))return;const e=state.events.find(e=>h.textContent===e.name);if(e)h.append(node(e,now));});
 const share=document.querySelector('.rf-share h2');if(share&&!share.querySelector('[data-event-state]')){const e=state.events.find(e=>share.textContent===e.name);if(e)share.append(node(e,now));}
}
function watch(getState,getView,getSelected){let queued=false;const run=()=>{queued=false;decorate(getState,getView,getSelected);};const schedule=()=>{if(!queued){queued=true;queueMicrotask(run);}};const observer=new MutationObserver(changes=>{if(changes.some(c=>[...c.addedNodes].some(n=>n.nodeType===1&&!n.matches?.('.ev-state,.ev-title-state'))))schedule();});observer.observe(document.body,{childList:true,subtree:true});setInterval(()=>{if(!document.hidden)run();},15000);document.addEventListener('visibilitychange',()=>{if(!document.hidden)run();});return run;}
const api={phase,label,node,update,decorate,watch};if(typeof module!=='undefined'&&module.exports)module.exports=api;else host.SpatialEventState=api;
})(typeof window==='undefined'?{}:window);
