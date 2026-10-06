use super::*;
use ui::esc;

fn icon(name: &str) -> String {
    let path = match name {
        "home" => "M3 10 12 3l9 7v11h-6v-7H9v7H3Z",
        "map" => "m3 5 6-2 6 2 6-2v16l-6 2-6-2-6 2Zm6-2v16m6-14v16",
        "graph" => "M9 6h6M7 8l-3 9m13-9 3 9M7 19h10M12 8v9M9 5a3 3 0 1 0 6 0 3 3 0 1 0-6 0M1 19a3 3 0 1 0 6 0 3 3 0 1 0-6 0m16 0a3 3 0 1 0 6 0 3 3 0 1 0-6 0",
        "agent" => "M12 3v3M7 6h10a4 4 0 0 1 4 4v7a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4v-7a4 4 0 0 1 4-4Zm1 6v2m8-2v2m-7 3h6",
        "saved" => "M5 3h14v19l-7-5-7 5Z",
        "profile" => "M8 7a4 4 0 1 0 8 0 4 4 0 1 0-8 0M3 22v-3a9 9 0 0 1 18 0v3",
        "glasses" => "M2 8h20v8a4 4 0 0 1-8 0v-2h-4v2a4 4 0 0 1-8 0Zm0 0 2-4m18 4-2-4",
        "search" => "M3 10a7 7 0 1 0 14 0 7 7 0 1 0-14 0m12 5 6 6",
        "pin" => "M19 10c0 5-7 12-7 12S5 15 5 10a7 7 0 0 1 14 0ZM9 10a3 3 0 1 0 6 0 3 3 0 1 0-6 0",
        "shield" => "m12 2 9 4v6c0 5-9 10-9 10S3 17 3 12V6Zm-5 9 3 3 7-7",
        _ => "M4 12h16m-6-6 6 6-6 6",
    };
    format!("<svg class='icon' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.7' stroke-linecap='round' stroke-linejoin='round' aria-hidden='true'><path d='{path}'/></svg>")
}
fn pref(c: &Connection, entity: &str, kind: &str) -> bool {
    c.query_row(
        "SELECT value='1' FROM preferences WHERE entity=?1 AND kind=?2",
        params![entity, kind],
        |r| r.get(0),
    )
    .unwrap_or(false)
}
fn action(token: &str, op: &str, entity: &str, active: bool, label: &str) -> String {
    format!("<form method='post' action='/action' class='inline-form'><input type='hidden' name='token' value='{}'><input type='hidden' name='key' value='{}'><input type='hidden' name='op' value='{op}'><input type='hidden' name='id' value='{}'><input type='hidden' name='value' value='{}'><button class='{}' aria-pressed='{active}'>{}</button></form>",esc(token),id(),esc(entity),if active{"0"}else{"1"},if active{"outline is-saved"}else{"outline"},esc(label))
}
fn art(e: &Event) -> String {
    format!("<div class='art art-{}' role='img' aria-label='{}のコンセプトビジュアル'><span class='art-orbit'></span><span class='art-caption'>{}</span></div>", e.kind.to_lowercase(),esc(&e.kind),match e.kind.as_str(){"ART"=>"LIGHT / FIELD", "MUSIC"=>"SOUND / WAVES", "AR"=>"BEYOND / REAL", _=>"WALK / TOKYO"})
}
fn card(c: &Connection, token: &str, e: &Event) -> String {
    let saved = pref(c, &e.id, "save");
    format!("<article class='spot-card'><a class='card-visual' href='/?selected={}'>{}<span class='type-badge'>{}</span></a><div class='card-body'><p class='micro'>{} · {:.0} m</p><a href='/?selected={}'><h3>{}</h3></a><p>{}</p><div class='card-bottom'><span>{}</span>{}</div></div></article>",e.id,art(e),esc(&e.kind),icon("pin"),distance(e.lat,e.lon)*1000.,e.id,esc(&e.name),esc(&e.place),esc(&e.time.replace('T'," · ")),action(token,"save",&e.id,saved,if saved{"保存済み"}else{"＋ 保存"}))
}
fn search(v: &View, tab: &str) -> String {
    let mut s=format!("<form class='discovery-search' method='get'><input type='hidden' name='tab' value='{tab}'>{}<input name='q' aria-label='イベントや場所を検索' placeholder='場所・イベント・気になることから探す' value='{}'><select name='radius' aria-label='探索半径'>",icon("search"),esc(&v.q));
    for (value, label) in [("2", "2 km以内"), ("1", "1 km以内"), ("0.5", "500 m以内")] {
        s.push_str(&format!(
            "<option value='{value}' {}>{label}</option>",
            if v.radius == value { "selected" } else { "" }
        ));
    }
    s.push_str(
        "</select><select name='kind' aria-label='カテゴリー'><option value=''>すべて</option>",
    );
    for k in ["ART", "MUSIC", "WALK", "AR"] {
        s.push_str(&format!(
            "<option {}>{k}</option>",
            if v.kind == k { "selected" } else { "" }
        ));
    }
    s.push_str("</select><button class='primary'>探す</button></form>");
    s
}
fn map(list: &[&Event], selected: &str) -> String {
    let mut s=String::from("<section class='spatial-map'><div class='map-label'><b>AKIHABARA</b><span>秋葉原駅を基点に探索</span></div><svg viewBox='0 0 800 480' role='img' aria-label='秋葉原周辺の模式地図。マーカーで詳細を開きます'><defs><pattern id='blocks' width='100' height='80' patternUnits='userSpaceOnUse'><rect x='10' y='10' width='72' height='52' rx='7' fill='#dae6ee'/><path d='M0 75H100M94 0V80' stroke='white' stroke-width='10'/></pattern></defs><rect width='800' height='480' fill='#eaf3f7'/><rect width='800' height='480' fill='url(#blocks)'/><path d='M-30 350Q160 230 370 310T850 240' fill='none' stroke='#87d7f5' stroke-width='35'/><path d='M340-30 440 510M-20 110 820 410' stroke='white' stroke-width='19'/><path d='M340-30 440 510' stroke='#acd1df' stroke-width='5' stroke-dasharray='8 7'/><path d='M20 20h135v95H20zM570 335h140v110H570z' fill='#b9ddca'/><circle cx='400' cy='240' r='145' fill='#0278ff' fill-opacity='.04' stroke='#71b1ff' stroke-dasharray='5 7'/><g fill='#5e7994' font-size='17'><text x='32' y='144'>御茶ノ水</text><text x='465' y='75'>秋葉原</text><text x='40' y='405'>神田</text><text x='662' y='240'>浅草橋</text><text x='100' y='292'>神田川</text></g><circle cx='400' cy='240' r='17' fill='#1683ff' fill-opacity='.2'/><circle cx='400' cy='240' r='8' fill='#0879ff' stroke='white' stroke-width='3'/>");
    for (i, e) in list.iter().enumerate() {
        let x = 400. + (e.lon - 139.7731) * 22000.;
        let y = 240. - (e.lat - 35.6984) * 24000.;
        if (20. ..780.).contains(&x) && (30. ..450.).contains(&y) {
            s.push_str(&format!("<a href='/?selected={}' aria-label='{}の詳細'><circle cx='{x}' cy='{y}' r='22' fill='{}' stroke='white' stroke-width='4'/><text x='{x}' y='{}' fill='white' text-anchor='middle' font-size='15' font-weight='700'>{}</text></a>",e.id,esc(&e.name),if selected==e.id{"#083b9b"}else{"#0784fa"},y+5.,i+1));
        }
    }
    s.push_str("</svg><p class='map-disclaimer'>模式図・直線距離 ／ 実際の道路や経路を示すものではありません</p></section>");
    s
}
fn companion() -> String {
    format!("<section class='companion'><div class='companion-heading'><div class='bot'>{}</div><div><h2>Spatial Guide</h2><span>次の発見を、いっしょに。</span></div><span class='online'></span></div><div class='greeting'>今日は、どんな街に出会いたい？<br>近くの体験から、あなたの一歩を見つけましょう。</div><a class='suggestion' href='/?tab=map&radius=0.5'>{} 徒歩圏の体験を探す <span>↗</span></a><a class='suggestion' href='/?tab=home&kind=ART'>{} アートに触れたい <span>↗</span></a><a class='suggestion' href='/?tab=glasses'>{} グラス表示を体験 <span>↗</span></a><div class='guide-foot'>検索・距離計算によるガイド<br>ぽよを開くと状況に応じた提案へ</div><a class='primary' href='/?tab=guide'>お出かけプランをつくる →</a></section><section class='trust-note'>{}<div><b>情報の背景まで、見える。</b><p>作成者・保存状態・更新履歴を確認して、自分のペースで探索。</p><a href='/?tab=ops'>実行の記録を見る →</a></div></section>",icon("agent"),icon("pin"),icon("search"),icon("glasses"),icon("shield"))
}
pub(super) fn render(c: &Connection, token: &str, v: &View) -> String {
    let all = events(c);
    let tab = if v.tab.is_empty() {
        "home"
    } else {
        v.tab.as_str()
    };
    let radius = v
        .radius
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite() && *x > 0.)
        .unwrap_or(2.);
    let list: Vec<&Event> = all
        .iter()
        .filter(|e| {
            (v.q.is_empty()
                || format!("{} {}", e.name, e.place)
                    .to_lowercase()
                    .contains(&v.q.to_lowercase()))
                && (v.kind.is_empty() || e.kind == v.kind)
                && distance(e.lat, e.lon) <= radius
        })
        .collect();
    let mut s=String::from("<!doctype html><html lang='ja'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width, initial-scale=1, viewport-fit=cover'><meta name='theme-color' content='#087bff'><meta name='description' content='人・場所・体験がつながる、Spatialの街歩き。'><title>Spatial — 好きな場所で、好きな未来を。</title><link rel='stylesheet' href='/style.css'><script src='/presence.js' defer></script><script src='/location.js' defer></script><script src='/device.js' defer></script></head><body><a class='skip-link' href='#main'>本文へ移動</a>");
    s.push_str(&format!("<header class='topbar'><a class='brand' href='/'><span class='brand-mark'>S</span>Spatial<span class='brand-sub'>A MORE CONNECTED WORLD</span></a><div class='header-location'>{} 東京・秋葉原 <span class='location-label'>探索の基点</span></div><a class='account-link' href='/?tab=profile'><span class='avatar'>凪</span><span>Nagi Studio</span></a></header><div class='app-shell'><aside class='sidebar'><div class='sidebar-label'>YOUR WORLD</div><nav aria-label='メインナビゲーション'>",icon("pin")));
    for (key, name) in [
        ("home", "ホーム / 発見"),
        ("map", "近くを探す"),
        ("graph", "つながり"),
        ("guide", "Spatial Guide"),
        ("saved", "保存した体験"),
        ("profile", "クリエイター"),
        ("glasses", "グラス体験"),
    ] {
        let i = if key == "guide" { "agent" } else { key };
        s.push_str(&format!(
            "<a href='/?tab={key}' class='{}' {}>{}<span>{name}</span></a>",
            if tab == key { "active" } else { "" },
            if tab == key {
                "aria-current='page'"
            } else {
                ""
            },
            icon(i)
        ));
    }
    s.push_str("</nav><div class='create-callout'><span>CREATE SOMETHING</span><h3>街に、あなたの<br>物語を。</h3><a href='/?tab=create'>＋ 体験をつくる</a></div><div class='workspace-links'><a href='/?tab=agent'>提案と承認</a><a href='/?tab=identity'>Identity 台帳</a><a href='/?tab=ops'>運用・監査</a></div><p class='sandbox-label'><span class='online'></span>プライベート体験環境<br>イベントはサンプルデータ</p></aside><main id='main'>");
    if v.notice == "done" {
        s.push_str(
            "<div class='notice' role='status'>✓ 保存しました。最新の状態を表示しています。</div>",
        );
    }
    if let Some(e) = all
        .iter()
        .find(|e| e.id == v.selected)
        .filter(|_| tab != "glasses")
    {
        s.push_str(&format!("<a class='back-link' href='/?tab=map'>← 探索に戻る</a><div class='detail-layout'><section class='detail-main'><div class='detail-visual'>{}</div><div class='paper'><span class='tag'>{} / SAMPLE EXPERIENCE</span><h1>{}</h1><p class='detail-lead'>人と街の、新しい接点を探しに。</p><div class='detail-facts'><p>◷ {} JST</p><p>{} {} · 基点から {:.0} m</p></div><div class='detail-actions'>{}{}<a class='outline' href='/calendar?selected={}'>カレンダーに追加</a><a class='primary' href='/?tab=glasses&selected={}'>グラスで体験 →</a></div><hr><h2>この体験について</h2><p>「{}」を通じて、{}の空間とつながる体験です。開催情報はこのワークスペースの作成者が入力しています。</p><p class='muted'>体験用のデータです。実在する催しやチケットの販売ではありません。「参加予定」は自分の予定への記録です。</p><h2>情報と信頼性</h2><dl class='provenance'><dt>情報源</dt><dd>Nagi Studio / ワークスペース入力</dd><dt>検証状態</dt><dd>外部の公式情報による検証なし</dd><dt>更新版</dt><dd>v{} · {}</dd><dt>Spatial ID</dt><dd><code>{}</code></dd></dl><a class='outline' href='/?tab=agent'>日時の変更を提案</a></div></section><aside class='detail-aside'><section class='paper creator-mini'><span class='avatar large'>凪</span><h2>Nagi Studio</h2><p>街に、意味を重ねる。</p><a class='primary' href='/?tab=profile'>クリエイターを見る →</a></section>{}</aside></div>",art(e),esc(&e.kind),esc(&e.name),esc(&e.time.replace('T'," ")),icon("pin"),esc(&e.place),distance(e.lat,e.lon)*1000.,action(token,"save",&e.id,pref(c,&e.id,"save"),if pref(c,&e.id,"save"){"保存済み · 解除"}else{"＋ 保存する"}),action(token,"join",&e.id,pref(c,&e.id,"join"),if pref(c,&e.id,"join"){"参加予定 · 取り消す"}else{"参加予定にする"}),e.id,e.id,esc(&e.name),esc(&e.place),e.version,esc(&e.status),e.id,companion()));
    } else {
        match tab {
            "home" | "explore" | "map" => {
                s.push_str("<div class='page-heading'><div><p class='eyebrow'>DISCOVER YOUR NEXT</p><h1>好きな場所で、好きな未来を。</h1></div><a class='subtle-link' href='/?tab=saved'>マイライブラリ ↗</a></div>");
                s.push_str(&search(v, tab));
                if tab != "map" && v.q.is_empty() && v.kind.is_empty() {
                    s.push_str("<section class='hero'><div class='hero-copy'><span class='hero-kicker'>PEOPLE. PLACES. POSSIBILITIES.</span><h2>いつもの街が、<br>もっとおもしろくなる。</h2><p>人・場所・体験がつながる。<br>現実とデジタルが重なる、新しい日常へ。</p><a class='primary' href='/?tab=map'>空間を探索する →</a></div><div class='hero-stamp'>TOKYO FIELD NOTES <b>01 / 秋葉原</b></div><span class='image-label'>コンセプトイメージ · AI生成</span></section>");
                }
                s.push_str("<div class='discovery-layout'><div>");
                if tab == "map" {
                    s.push_str("<div class='view-switch'><a class='active' href='/?tab=map'>地図</a><a href='/?tab=graph'>グラフ</a><a href='/?tab=home'>リスト</a></div>");
                    s.push_str(&map(&list, &v.selected));
                }
                s.push_str(&format!("<div class='section-heading'><div><p class='eyebrow'>NEAR YOU</p><h2>近くの、気になる体験</h2></div><span>{} 件</span></div><div class='card-grid'>",list.len()));
                for e in &list {
                    s.push_str(&card(c, token, e));
                }
                if list.is_empty() {
                    s.push_str("<div class='empty'><h2>まだ見つかりませんでした</h2><p>キーワードや距離を変えて、もう一度。</p><a class='primary' href='/'>条件をリセット</a></div>");
                }
                s.push_str("</div><a class='world-banner' href='/?tab=glasses'><div><span>BEYOND THE SCREEN</span><h2>見る世界に、発見を重ねる。</h2><p>スマートグラスの表示をプレビュー。</p></div>");
                s.push_str(&icon("glasses"));
                s.push_str("</a></div><aside class='right-rail'>");
                s.push_str(&companion());
                s.push_str("</aside></div>");
            }
            "saved" => {
                let saved: Vec<_> = all
                    .iter()
                    .filter(|e| pref(c, &e.id, "save") || pref(c, &e.id, "join"))
                    .collect();
                s.push_str(&format!("<div class='page-heading'><div><p class='eyebrow'>YOUR COLLECTION</p><h1>次に行きたい、を集めよう。</h1><p>保存した体験と参加予定 · {} 件</p></div></div><div class='card-grid library'>",saved.len()));
                for e in saved {
                    s.push_str("<div>");
                    if pref(c, &e.id, "join") {
                        s.push_str("<p class='tag'>✓ 参加予定</p>");
                    }
                    s.push_str(&card(c, token, e));
                    s.push_str("</div>");
                }
                if !all
                    .iter()
                    .any(|e| pref(c, &e.id, "save") || pref(c, &e.id, "join"))
                {
                    s.push_str("<section class='paper empty'><h2>あなたのコレクションは、ここから。</h2><p>気になる体験の「保存」を押すと、ここに集まります。</p><a class='primary' href='/'>体験を探す →</a></section>");
                }
                s.push_str("</div>");
            }
            "profile" => {
                s.push_str(&format!("<section class='profile-cover'><p>PEOPLE MAKE PLACES.</p><h1>街に、意味を重ねる。</h1></section><section class='paper profile-info'><span class='avatar large'>凪</span><div><p class='eyebrow'>@NAGI / CREATOR</p><h2>Nagi Studio</h2><p>風景、音、光。日常に隠れた小さな発見を、空間の体験に。</p><span class='muted'>サンプルのクリエイター · 外部認証なし</span></div>{}</section><div class='section-heading'><h2>公開した体験</h2><span>{} 作品</span></div><div class='card-grid library'>",action(token,"follow","creator-local",pref(c,"creator-local","follow"),if pref(c,"creator-local","follow"){"フォロー中 · 解除"}else{"＋ フォローする"}),all.len()));
                for e in &all {
                    s.push_str(&card(c, token, e));
                }
                s.push_str("</div>");
            }
            "guide" => {
                s.push_str(&format!("<div class='page-heading'><div><p class='eyebrow'>SPATIAL GUIDE</p><h1>今日の一歩を、一緒に探そう。</h1></div></div><section class='paper planner'><div class='bot'>{}</div><h2>どんな体験を探していますか？</h2><p>場所・イベント名、カテゴリーと距離から候補を選びます。</p>{}<p class='muted'>保存済みデータの検索と直線距離によるプランです。生成AIの応答や道路の経路案内ではありません。</p></section><div class='section-heading'><h2>近い順のお出かけ候補</h2><a href='/?tab=agent'>イベントの変更を相談 →</a></div><div class='itinerary'>",icon("agent"),search(v,"guide")));
                let mut nearby = list.clone();
                nearby.sort_by(|a, b| distance(a.lat, a.lon).total_cmp(&distance(b.lat, b.lon)));
                for (n, e) in nearby.iter().take(3).enumerate() {
                    s.push_str(&format!("<div class='itinerary-step'><span>{:02}</span><p>基点から {:.0} m</p>{}</div>",n+1,distance(e.lat,e.lon)*1000.,card(c,token,e)));
                }
                if nearby.is_empty() {
                    s.push_str("<p class='empty'>条件に合う候補がありません。検索条件を広げてください。</p>");
                }
                s.push_str("</div>");
            }
            "glasses" => {
                let e = all
                    .iter()
                    .find(|e| e.id == v.selected)
                    .or_else(|| all.first());
                s.push_str("<div class='page-heading'><div><p class='eyebrow'>SPATIAL LENS / PREVIEW</p><h1>見る世界に、そっと重なる。</h1><p>グラス用HUDのシミュレーター。実機やカメラは使わず、視界への情報表示を体験できます。</p></div></div><section class='lens'><div class='lens-top'><span>SPATIAL LENS</span><span>東京・秋葉原 / シーン再現</span></div>");
                if let Some(e) = e {
                    let idx = all.iter().position(|x| x.id == e.id).unwrap_or(0);
                    let next = &all[(idx + 1) % all.len()];
                    if v.kind != "focus" {
                        s.push_str(&format!("<a class='lens-anchor' href='/?selected={}'><span class='anchor-pin'>{}</span><div><span>{} · コンセプト表示</span><h2>{}</h2><p>{} · 直線 {:.0} m</p><b>詳細を開く ↗</b></div></a><div class='lens-trust'>{} ワークスペース入力 / 公式確認なし</div>",e.id,icon("pin"),esc(&e.kind),esc(&e.name),esc(&e.place),distance(e.lat,e.lon)*1000.,icon("shield")));
                    } else {
                        s.push_str("<div class='focus-label'>視界に集中しています</div>");
                    }
                    s.push_str(&format!("<div class='lens-controls'><a href='/?tab=glasses&selected={}&kind={}'>{} {}</a><a href='/?tab=glasses&selected={}'>{} 次のアンカー</a><a href='/?selected={}'>{} 詳細へ</a></div>",e.id,if v.kind=="focus"{""}else{"focus"},icon("glasses"),if v.kind=="focus"{"情報を表示"}else{"情報を隠す"},next.id,icon("map"),e.id,icon("profile")));
                }
                s.push_str("<span class='image-label'>AI生成の背景 / 実際の視界ではありません</span></section><div class='lens-explain'><section class='paper'><h3>01 · 視界を邪魔しない</h3><p>情報を隠して、必要なときだけ再表示。キーボードでも操作できます。</p></section><section class='paper'><h3>02 · 気になる場所を選ぶ</h3><p>「次のアンカー」で切り替え、同じSpatial IDの詳細へつながります。</p></section><section class='paper'><h3>03 · 情報源を確認する</h3><p>直線距離と情報源を表示。ルート案内や公式認証と取り違えない表示です。</p></section></div>");
            }
            _ => {
                let legacy = ui::render(c, token, v);
                if let Some(content) = legacy
                    .split("<main>")
                    .nth(1)
                    .and_then(|x| x.split("</main>").next())
                {
                    s.push_str(content);
                }
            }
        }
    }
    s.push_str("<footer class='site-footer'><span>Spatial <b>A More Connected World</b></span><span>サンプル環境 · Rust runtime · JST</span><span><a href='/?tab=create'>体験をつくる</a> · <a href='/?tab=agent'>提案と承認</a> · <a href='/?tab=identity'>ID台帳</a> · <a href='/?tab=ops'>運用・監査</a></span></footer></main></div><nav class='mobile-nav' aria-label='モバイルナビゲーション'>");
    for (key, name, i) in [
        ("home", "ホーム", "home"),
        ("map", "マップ", "pin"),
        ("guide", "ガイド", "agent"),
        ("saved", "保存", "saved"),
        ("profile", "マイページ", "profile"),
    ] {
        s.push_str(&format!(
            "<a class='{}' href='/?tab={key}' {}>{}<span>{name}</span></a>",
            if tab == key { "active" } else { "" },
            if tab == key {
                "aria-current='page'"
            } else {
                ""
            },
            icon(i)
        ));
    }
    s.push_str("</nav>");
    s.push_str(&include_str!("../assets/companion.html").replace("{{TOKEN}}", &esc(token)));
    s.push_str("</body></html>");
    s
}
fn ical_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "")
        .replace(';', "\\;")
        .replace(',', "\\,")
}
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut n = 0;
    for ch in line.chars() {
        if n + ch.len_utf8() > 73 {
            out.push_str("\r\n ");
            n = 1;
        }
        out.push(ch);
        n += ch.len_utf8();
    }
    out
}
pub(super) async fn calendar(State(a): State<App>, Query(v): Query<View>) -> Response {
    let c = a.db.lock().unwrap();
    let Some(e) = events(&c).into_iter().find(|e| e.id == v.selected) else {
        return (StatusCode::NOT_FOUND, "イベントが見つかりません").into_response();
    };
    let stamp: String = c
        .query_row("SELECT strftime('%Y%m%dT%H%M%SZ','now')", [], |r| r.get(0))
        .unwrap();
    let lines = vec![
        "BEGIN:VCALENDAR".into(),
        "VERSION:2.0".into(),
        "PRODID:-//Spatial//Discovery//JA".into(),
        "CALSCALE:GREGORIAN".into(),
        "BEGIN:VTIMEZONE".into(),
        "TZID:Asia/Tokyo".into(),
        "BEGIN:STANDARD".into(),
        "DTSTART:19700101T000000".into(),
        "TZOFFSETFROM:+0900".into(),
        "TZOFFSETTO:+0900".into(),
        "TZNAME:JST".into(),
        "END:STANDARD".into(),
        "END:VTIMEZONE".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{}@spatial.local", e.id),
        format!("DTSTAMP:{stamp}"),
        format!(
            "DTSTART;TZID=Asia/Tokyo:{}00",
            e.time.replace(['-', ':'], "")
        ),
        format!("SUMMARY:{}", ical_text(&e.name)),
        format!("LOCATION:{}", ical_text(&e.place)),
        "DESCRIPTION:Spatialサンプル体験。実在の催しではありません。".into(),
        format!("SEQUENCE:{}", e.version),
        "END:VEVENT".into(),
        "END:VCALENDAR".into(),
    ];
    (
        [
            ("content-type", "text/calendar; charset=utf-8"),
            (
                "content-disposition",
                "attachment; filename=spatial-event.ics",
            ),
            ("cache-control", "no-store"),
        ],
        format!(
            "{}\r\n",
            lines
                .iter()
                .map(|x| fold(x))
                .collect::<Vec<_>>()
                .join("\r\n")
        ),
    )
        .into_response()
}
