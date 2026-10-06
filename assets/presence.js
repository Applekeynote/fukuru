'use strict';
(() => {
const root=document.querySelector('#poyo-root');if(!root)return;
const $=s=>document.querySelector(s),bubble=$('#poyo-bubble'),panel=$('#poyo-panel'),dialog=$('#poyo-confirm');
let settings={speak:false,roam:true};try{settings={...settings,...JSON.parse(sessionStorage.getItem('poyo-presence')||'{}')};}catch{}
let proposal=null,lastText='',perch=0,held=false,epoch=0,voiceEpoch=0;
const reduced=matchMedia('(prefers-reduced-motion: reduce)');
const save=()=>{try{sessionStorage.setItem('poyo-presence',JSON.stringify(settings));}catch{}};
function controls(){$('#poyo-speaker').textContent='発話：'+(settings.speak?'オン':'オフ');$('#poyo-speaker').setAttribute('aria-pressed',String(settings.speak));$('#poyo-roam').textContent='おさんぽ：'+(settings.roam?'オン':'オフ');$('#poyo-roam').setAttribute('aria-pressed',String(settings.roam));}
function silence(){voiceEpoch++;window.speechSynthesis?.cancel();root.classList.remove('poyo-talking');}
async function speak(text){if(!settings.speak||document.hidden)return;silence();const ticket=voiceEpoch;let voice=window.speechSynthesis?.getVoices().find(v=>v.localService&&v.lang.toLowerCase().startsWith('ja'));if(!voice&&window.speechSynthesis){await new Promise(resolve=>{const done=()=>{clearTimeout(timer);speechSynthesis.removeEventListener('voiceschanged',done);resolve();};const timer=setTimeout(done,1500);speechSynthesis.addEventListener('voiceschanged',done,{once:true});});if(ticket!==voiceEpoch||!settings.speak||document.hidden)return;voice=speechSynthesis.getVoices().find(v=>v.localService&&v.lang.toLowerCase().startsWith('ja'));}if(!voice){$('#poyo-status').textContent='端末内の日本語音声がないため、吹き出しでお知らせします。';return;}const u=new SpeechSynthesisUtterance(text.slice(0,180));u.voice=voice;u.lang='ja-JP';u.rate=1.05;u.pitch=1.18;u.onstart=()=>{root.classList.add('poyo-talking');$('#poyo-status').textContent='端末の日本語音声で話しています。';};u.onend=()=>{root.classList.remove('poyo-talking');$('#poyo-status').textContent='読み上げが終わりました。';};u.onerror=()=>{root.classList.remove('poyo-talking');$('#poyo-status').textContent='読み上げを利用できないため、吹き出しでお知らせします。';};speechSynthesis.speak(u);}
function comment(text){lastText=String(text);$('#poyo-bubble-text').textContent=lastText.length>180?lastText.slice(0,177)+'…':lastText;bubble.hidden=!panel.hidden;speak(lastText);}
$('#poyo-speaker').onclick=()=>{settings.speak=!settings.speak;save();controls();if(settings.speak)speak('発話をオンにしました。いつでもオフにできます。');else silence();};
$('#poyo-roam').onclick=()=>{settings.roam=!settings.roam;save();controls();if(!settings.roam)dock();};
$('#poyo-bubble-close').onclick=()=>bubble.hidden=true;
$('#poyo-bubble-plan').onclick=()=>{$('#poyo-toggle').click();$('#poyo-screen-actions').scrollIntoView({block:'nearest'});};
function dock(){root.dataset.perch='0';root.getAnimations().forEach(a=>a.finish());}
function wander(){if(!settings.roam||reduced.matches||root.classList.contains('quiet-motion')){dock();return;}if(document.hidden||held||!panel.hidden||dialog.open)return;perch=(perch+1)%4;root.dataset.perch=String(perch);}
root.addEventListener('pointerenter',()=>{held=true;root.getAnimations().forEach(a=>a.pause());});
root.addEventListener('pointerleave',()=>{held=false;root.getAnimations().forEach(a=>a.play());});
root.addEventListener('focusin',()=>{held=true;root.getAnimations().forEach(a=>a.pause());});root.addEventListener('focusout',()=>{held=false;root.getAnimations().forEach(a=>a.play());});
new MutationObserver(()=>{if(!panel.hidden){dock();bubble.hidden=true;}}).observe(panel,{attributes:true,attributeFilter:['hidden']});
reduced.addEventListener('change',()=>{if(reduced.matches)dock();});
setInterval(wander,18000);setTimeout(wander,7000);controls();
root.addEventListener('poyo-comment',e=>comment(e.detail.text));
root.addEventListener('poyo-candidates',e=>{document.querySelectorAll('[data-poyo-candidate]').forEach(x=>x.remove());const id=e.detail.ids[0];if(!id)return;for(const [action,label] of [['open_event','おすすめの詳細を開く提案'],['save_event','おすすめを保存する提案'],['join_event','参加予定にする提案']]){const b=document.createElement('button');b.type='button';b.dataset.poyoCandidate='true';b.dataset.poyoAction=action;b.dataset.entity=id;b.textContent=label;$('#poyo-screen-actions').append(b);}});
async function api(path,body){const response=await fetch('/api/ui/'+path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token:root.dataset.token,...body}),signal:AbortSignal.timeout(15000)});const data=await response.json();if(!response.ok)throw Error(data.error||'操作を確認できませんでした。');return data;}
$('#poyo-screen-actions').onclick=async e=>{const b=e.target.closest('[data-poyo-action]');if(!b||proposal)return;const runEpoch=epoch;b.disabled=true;try{const p=await api('propose',{action:b.dataset.poyoAction,entity:b.dataset.entity||''});if(epoch!==runEpoch)return;proposal=p;$('#poyo-confirm-description').textContent=p.label;$('#poyo-confirm-status').textContent='';$('#poyo-confirm-run').disabled=false;dialog.showModal();$('#poyo-confirm-cancel').focus();dock();}catch(error){$('#poyo-status').textContent=error.message;}finally{b.disabled=false;}};
async function cancel(){const p=proposal;proposal=null;dialog.close();if(p)try{await api('decide',{id:p.id,confirm:false});}catch{}comment('今回は操作せずに、そのままにしておくね。');}
$('#poyo-confirm-cancel').onclick=cancel;dialog.addEventListener('cancel',e=>{e.preventDefault();cancel();});
$('#poyo-confirm-run').onclick=async()=>{if(!proposal)return;$('#poyo-confirm-run').disabled=true;try{const data=await api('decide',{id:proposal.id,confirm:true});proposal=null;dialog.close();comment(data.message);if(data.url){const url=new URL(data.url,location.origin);if(url.origin===location.origin&&url.pathname==='/')location.assign(url.href);}}catch(error){$('#poyo-confirm-status').textContent=error.message+' 通信が途切れた場合は、保存一覧などで結果を確認してください。';}};
$('#poyo-clear').addEventListener('click',()=>{epoch++;silence();if(proposal){const p=proposal;proposal=null;api('decide',{id:p.id,confirm:false}).catch(()=>{});}dialog.close();settings={speak:false,roam:true};save();controls();dock();});
document.addEventListener('visibilitychange',()=>{if(document.hidden)silence();});window.addEventListener('pagehide',silence);
})();
