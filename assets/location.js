'use strict';
// Browser platform adapter; the controller can be tested without inventing a device fix.
(function(host){
function errorMessage(error,options={}){
 const code=Number(error?.code),agent=options.userAgent||host.navigator?.userAgent||'';
 const iphone=/iPhone|iPad|iPod/i.test(agent);
 const standalone=options.standalone??(host.navigator?.standalone===true||host.matchMedia?.('(display-mode: standalone)').matches===true);
 if(code===1){
  if(iphone&&standalone)return 'ホーム画面のSpatialで位置情報が拒否されました。iPhoneの「設定 → プライバシーとセキュリティ → 位置情報サービス」でSpatial（表示されなければSafariのWebサイト）の許可と、サイトの位置情報許可を確認し、この画面で再試行してください。Chromeの許可だけではホーム画面アプリに反映されない場合があります。地域名の入力でも探せます。';
  if(iphone)return 'iPhoneで位置情報が拒否されました。「設定 → プライバシーとセキュリティ → 位置情報サービス」で、この画面を開いているブラウザの許可とサイトの位置情報許可を確認し、再試行してください。地域名の入力でも探せます。';
  return '位置情報が拒否されました。ブラウザのサイト権限と端末の位置情報サービスを確認し、再試行してください。地域名の入力でも探せます。';
 }
 if(code===3)return '位置の取得が時間切れになりました。通信や端末の位置情報サービスを確認して再試行するか、地域名を入力してください。';
 return '端末から位置を取得できませんでした。位置情報サービスを確認して再試行するか、地域名を入力してください。';
}
function create(options){
 const {geo,permissions,onFix,onStatus,onError}=options;
 let generation=0,watch=null,active=false,permission='unknown',permissionStatus=null;
 function stop(clear=true){generation++;active=false;if(watch!==null)geo.clearWatch(watch);watch=null;if(permissionStatus)permissionStatus.onchange=null;if(clear)onFix(null);}
 function attempt(high){const ticket=++generation;active=true;
  if(watch!==null)geo.clearWatch(watch);
  onStatus({permission,phase:high?'high-accuracy':'standard',active:true});
  watch=geo.watchPosition(p=>{
   if(!active||ticket!==generation)return;
   const {latitude:lat,longitude:lon,accuracy}=p.coords;
   if(!Number.isFinite(lat)||!Number.isFinite(lon)||!Number.isFinite(accuracy)||Math.abs(lat)>90||Math.abs(lon)>180||accuracy<0){stop();onError({code:2,permission,reason:'invalid-fix'});return;}
   onFix({lat,lon,accuracy,timestamp:p.timestamp,source:'device',speed:Number.isFinite(p.coords.speed)&&p.coords.speed>=0?p.coords.speed:null,heading:Number.isFinite(p.coords.heading)?p.coords.heading:null});onStatus({permission,phase:'tracking',active:true});
  },error=>{
   if(!active||ticket!==generation)return;
   if(!high&&[2,3].includes(error.code)){attempt(true);return;}
   stop();onError({code:error.code,permission,reason:'provider'});
  },{enableHighAccuracy:high,maximumAge:high?0:30000,timeout:high?35000:10000});
 }
 function start(){if(active){stop();onStatus({permission,phase:'stopped',active:false});return;}
  attempt(false);const current=generation;
  if(permissions?.query)permissions.query({name:'geolocation'}).then(p=>{if(!active||current!==generation)return;permissionStatus=p;permission=p.state;onStatus({permission,phase:'waiting-for-fix',active:true});p.onchange=()=>{permission=p.state;onStatus({permission,phase:'waiting-for-fix',active});};}).catch(()=>{});
 }
 return {start,stop,isActive:()=>active};
}
if(typeof module!=='undefined'&&module.exports)module.exports={create,errorMessage};else host.SpatialLocation={create,errorMessage};
})(typeof window==='undefined'?{}:window);
