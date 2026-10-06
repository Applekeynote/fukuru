"""Build deterministic fictional records; never calls AI, GPS, or external APIs."""
import json, re
from pathlib import Path
from datetime import datetime, timedelta, timezone

ROOT = Path(__file__).resolve().parents[1]
prefectures = json.loads(re.search(r"const prefectures=(\[.*?\]);", (ROOT/'assets/refinement.js').read_text(encoding='utf-8')).group(1).replace("'", '"'))
points = json.loads(re.search(r"const regionPoints=(\[.*?\]);", (ROOT/'assets/spatial.js').read_text(encoding='utf-8')).group(1).replace("'", '"'))
points = {p[0]: p[1:] for p in points}
cities = ['札幌','青森','盛岡','仙台','秋田','山形','福島','水戸','宇都宮','前橋','さいたま','千葉','東京','横浜','新潟','富山','金沢','福井','甲府','長野','岐阜','静岡','名古屋','津','大津','京都','大阪','神戸','奈良','和歌山','鳥取','松江','岡山','広島','山口','徳島','高松','松山','高知','福岡','佐賀','長崎','熊本','大分','宮崎','鹿児島','那覇']
seasonal = {
 10: [('秋色フォトさんぽ','写真','街並み',False,0,'カメラやスマートフォンで、秋の色を見つけながら街を歩くひととき。'),('手づくり紙ものの会','クラフト','ものづくりスペース',True,500,'便せんや小さなカードを持ち寄り、秋の紙ものを作ります。'),('夕暮れスケッチ','アート','散策エリア',False,0,'夕暮れの景色を、好きな画材で一枚のスケッチに。'),('読書と珈琲の午後','読書','読書スペース',True,600,'読みかけの本と一緒に、静かな午後を過ごします。'),('秋のまち音めぐり','まち歩き','街並み',False,0,'風や足音、お店の音をたよりに、いつもの街をゆっくり歩きます。')],
 11: [('紅葉を探す朝さんぽ','まち歩き','散策エリア',False,0,'色づく木々や道の表情を見つける、短い朝の散歩。'),('冬支度のリースづくり','クラフト','ものづくりスペース',True,800,'枝や紙を組み合わせ、小さな冬のリースを作ります。'),('街の灯りフォトウォーク','写真','街並み',False,0,'灯りがともる時間に合わせ、街角の写真を撮り歩きます。'),('持ち寄りボードゲーム','交流','交流スペース',True,300,'初めての人も混ざって遊べる、気軽なボードゲームの集まり。'),('小さな朗読とお茶の会','読書','読書スペース',True,500,'お気に入りの短い文章を持ち寄り、お茶と一緒に楽しみます。')],
 12: [('冬の光を撮る散歩','写真','散策エリア',False,0,'低い冬の光や影を探しながら、街をゆっくり巡ります。'),('手づくり冬のカード','クラフト','ものづくりスペース',True,500,'大切な人へ渡すカードを、紙やスタンプで作ります。'),('年末のまち歩き','まち歩き','街並み',False,0,'一年の終わりに、街の小さな変化を見つける散歩。'),('本の交換とお茶の時間','読書','読書スペース',True,500,'誰かに読んでほしい一冊を持ち寄り、本を交換します。'),('今年の一枚 写真の会','写真','交流スペース',True,0,'今年撮ったお気に入りの写真を一枚選び、思い出を話します。')],
}
accounts, events = [], []
profiles = {p['id']: p for p in json.loads((ROOT/'data/regional-profiles-v15.json').read_text(encoding='utf-8'))}
for code, (pref, city) in enumerate(zip(prefectures,cities),1):
    owner = 'OWNER' if pref=='愛知県' else f'demo_region_{code:02}'
    if owner!='OWNER':
        accounts.append(dict(id=owner,handle=f'fukuru_{code:02}',name=f'{city}よりみち部',bio=f'{pref}の写真・まち歩き・ものづくり。',sample=True,disabled=True,password_hash='',ui_locale='ja',interests=['写真','まち歩き'],theme='system',layout='standard',seed_batch='prefectures-2026-q4-v13'))
        accounts[-1].update({k:v for k,v in profiles[owner].items() if k not in ('id','handle')})
    key = pref if pref=='北海道' else pref[:-1]
    lat, lon = points[key]
    for month, templates in seasonal.items():
        for n,(title,kind,venue,indoor,price,body) in enumerate(templates,1):
            start=datetime(2026,month,8+n*4,10+(n%3)*2,tzinfo=timezone(timedelta(hours=9)))
            eid=f'spid_region_2026_{code:02}_{month:02}_{n}'
            e=dict(id=eid,owner=owner,name=f'{city}｜{title}',place=f'{pref} {city}・{venue}',address=f'{pref} {city}',meeting='',description=body,kind=kind,start=start.isoformat(),end=(start+timedelta(hours=1)).isoformat(),lat=lat,lon=lon,floor=None,entrance='',location_precision='approximate',bring='歩きやすい靴、水分' if not indoor else '筆記用具',indoor=indoor,price_yen=price,capacity=None,venue_rights=False,venue_verified=False,status='active',version=1,sample=True,seed_batch='prefectures-2026-q4-v13')
            extra=dict(visibility='public',mode='onsite',languages=['ja'],cohosts=[],cover='',cover_alt='',weather_policy='屋外は雨天中止',duration_minutes=60,late_join=True)
            events.append(dict(event=e,extra=extra))
output=ROOT/'data/regional-events-2026-q4.json'
output.parent.mkdir(exist_ok=True)
output.write_text(json.dumps(dict(batch='prefectures-2026-q4-v13',owner_handle='rynat',accounts=accounts,events=events),ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
assert len(accounts)==46 and len(events)==705
print(f'{len(accounts)} accounts / {len(events)} events -> {output}')
