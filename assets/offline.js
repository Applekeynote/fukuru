const CACHE='spatial-offline-v2',STOP_AT=1798729200000;
self.addEventListener('install',e=>{e.waitUntil(caches.open(CACHE).then(c=>c.add('/offline')));self.skipWaiting();});
self.addEventListener('activate',e=>e.waitUntil(self.clients.claim()));
self.addEventListener('fetch',e=>{if(Date.now()>=STOP_AT){e.respondWith(Promise.resolve(new Response('サービスの提供期間が終了しました',{status:503,headers:{'Content-Type':'text/plain; charset=utf-8'}})));return;}if(e.request.method==='GET'&&new URL(e.request.url).pathname==='/offline')e.respondWith(fetch(e.request).catch(()=>caches.match('/offline')));});
self.addEventListener('push',e=>Date.now()<STOP_AT&&e.waitUntil(self.registration.showNotification('Spatialのお知らせ',{body:'予定変更や選んだ会話の更新があります。受信箱をご確認ください。',icon:'/poyo-owl.svg',tag:'spatial-inbox',data:{url:'/?view=alerts'}})));
self.addEventListener('notificationclick',e=>{e.notification.close();e.waitUntil(self.clients.openWindow('/?view=alerts'));});
