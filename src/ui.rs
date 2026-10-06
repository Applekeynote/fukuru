use super::*;

fn delivery_panel(c: &Connection, token: &str) -> String {
    let delivered: i64 = c
        .query_row(
            "SELECT count(*) FROM managed_deliveries WHERE status='DELIVERED'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let pending: i64 = c
        .query_row(
            "SELECT count(*) FROM managed_deliveries WHERE status IN ('QUEUED','LEASED','RETRY')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mut html=format!("<section class='paper delivery-panel'><div class='section-label'>CLOUD DELIVERY <span>反映済み {delivered} · 待機 {pending}</span></div><h2>街の記録を、クラウドへ。</h2><p>Spannerへの保存とAlloyDBの受領を確認した変更だけを反映済みにします。DB停止中は待機します。</p><div class='table-wrap'><table><tr><th>イベント</th><th>版</th><th>状態</th><th>試行</th><th>操作</th></tr>");
    let mut statement=c.prepare("SELECT d.event_id,COALESCE(e.name,d.entity_id),d.entity_version,d.status,d.attempts FROM managed_deliveries d LEFT JOIN events e ON e.id=d.entity_id ORDER BY d.rowid DESC LIMIT 50").unwrap();
    let mut count = 0;
    for row in statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .unwrap()
    {
        let (id, name, version, status, attempts) = row.unwrap();
        count += 1;
        let label = match status.as_str() {
            "DELIVERED" => "反映済み",
            "SUPERSEDED" => "新しい版を優先",
            "LEASED" => "反映中",
            "RETRY" => "再試行待ち",
            "DEAD" => "要確認",
            _ => "起動待ち",
        };
        let retry = if status == "DEAD" {
            format!("{}<input type='hidden' name='id' value='{}'><button class='outline'>再試行に戻す</button></form>",start(token,"retry_delivery"),esc(&id))
        } else {
            String::new()
        };
        html.push_str(&format!("<tr><td>{}</td><td>v{version}</td><td><span class='tag'>{label}</span></td><td>{attempts}/5</td><td>{retry}</td></tr>",esc(&name)));
    }
    if count == 0 {
        html.push_str(
            "<tr><td colspan='5'>新しく作成・承認したイベントがここに並びます。</td></tr>",
        );
    }
    html.push_str("</table></div><p class='muted'>検証ワーカーは自動で有料リソースを起動しません。保存・参加予定はこのアプリに保存します。</p></section>");
    html
}
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn start(token: &str, op: &str) -> String {
    format!("<form method='post' action='/action'><input type='hidden' name='token' value='{}'><input type='hidden' name='key' value='{}'><input type='hidden' name='op' value='{op}'>",esc(token),id())
}
pub(super) fn render(c: &Connection, token: &str, v: &View) -> String {
    let all = events(c);
    let radius = v
        .radius
        .parse::<f64>()
        .ok()
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or(2.0);
    let list: Vec<_> = all
        .iter()
        .filter(|e| {
            (v.q.is_empty()
                || format!("{} {}", e.name, e.place)
                    .to_lowercase()
                    .contains(&v.q.to_lowercase()))
                && (v.kind.is_empty() || v.kind == e.kind)
                && distance(e.lat, e.lon) <= radius
        })
        .collect();
    let pending: i64 = c
        .query_row(
            "SELECT count(*) FROM approvals WHERE status='PENDING'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let tab = if v.tab.is_empty() { "explore" } else { &v.tab };
    let mut body = String::new();
    body.push_str(&format!("<header><a class='brand' href='/'><span class='logo'>◈</span> ATLAS<span class='brand-sub'>SPATIAL NETWORK</span></a><div class='workspace'>TOKYO / 秋葉原 <span class='dot'></span></div><span class='env'>PRIVATE SANDBOX</span><span class='avatar'>凪</span></header><div class='shell'><aside><div class='small'>WORKSPACE / 01</div><nav>"));
    for (key, symbol, label) in [
        ("explore", "◎", "空間を探索"),
        ("graph", "⌘", "つながり"),
        ("identity", "◇", "Spatial Identity"),
        ("agent", "✳", "Agent Studio"),
        ("ops", "▥", "運用・監査"),
    ] {
        body.push_str(&format!(
            "<a class='{}' href='/?tab={key}'><span>{symbol}</span>{label}{}</a>",
            if tab == key { "active" } else { "" },
            if key == "agent" {
                format!("<i>{pending}</i>")
            } else {
                String::new()
            }
        ));
    }
    body.push_str("</nav><div class='side-note'><span class='small'>YOUR SPATIAL ID</span><p>凪 / Nagi Studio</p><span class='tag'>CREATOR</span><p class='muted'>街に、意味を重ねる。</p></div><div class='side-bottom'><span class='dot'></span> Rust runtime<br><small>開発体験 / サンプルデータ</small></div></aside><main>");
    if v.notice == "done" {
        body.push_str("<div class='notice' role='status'>✓ 操作を保存しました。最新の状態を表示しています。</div>")
    }
    let (title, subtitle) = match tab {
        "graph" => (
            "街は、つながりでできている。",
            "RELATIONSHIP GRAPH / 関係をたどる",
        ),
        "identity" => (
            "ひとつの存在に、ひとつのID。",
            "SPATIAL IDENTITY / 存在の台帳",
        ),
        "agent" => ("意図を、確かなアクションへ。", "AGENT STUDIO / 提案と承認"),
        "ops" => ("見えることが、信頼になる。", "OBSERVABILITY / 実行の記録"),
        _ => (
            "まだ知らない街に、出会う。",
            "SPATIAL EXPLORER / 街の観測室",
        ),
    };
    body.push_str(&format!("<div class='page-title'><div><div class='eyebrow'>{subtitle}</div><h1>{title}</h1></div><a class='primary' href='/?tab=create'>＋ 空間にイベントをつくる</a></div>"));
    match tab {
        "create" => {
            body.push_str(&format!("<section class='paper narrow'><div class='eyebrow'>NEW SPATIAL EVENT</div><h2>街に、新しいきっかけを。</h2><p class='muted'>作成したイベントは位置同期後に公開状態になります。</p>{}<label>イベント名<input name='name' required maxlength='100' placeholder='例：秋葉原の光を集める'></label><label>場所<input name='place' required maxlength='100' placeholder='秋葉原・万世橋'></label><div class='two'><label>緯度<input name='lat' type='number' step='any' min='-90' max='90' value='35.6984' required></label><label>経度<input name='lon' type='number' step='any' min='-180' max='180' value='139.7731' required></label></div><div class='two'><label>カテゴリー<select name='kind'><option>ART</option><option>MUSIC</option><option>WALK</option><option>AR</option></select></label><label>開催日時（日本時間）<input type='datetime-local' name='time' required></label></div><button class='primary'>イベントを作成 →</button></form></section>",start(token,"create")));
        }
        "graph" => {
            body.push_str("<section class='paper'><div class='section-label'>CREATOR → CREATED → EVENT <span>保存済みの関係</span></div><div class='graph'><div class='graph-source'><span class='avatar'>凪</span><h2>Nagi Studio</h2><code>creator-local</code></div><div class='graph-lines'>");
            for e in &all {
                body.push_str(&format!("<a class='graph-node' href='/?selected={}'><span class='relation'>CREATED ──────</span><span><b>{}</b><small>{} · {}</small></span></a>",e.id,esc(&e.name),e.kind,esc(&e.place)))
            }
            body.push_str("</div></div><p class='muted'>作成したイベントも、このグラフに追加されます。距離は位置情報から別途計算します。</p></section>")
        }
        "identity" => {
            body.push_str("<section class='paper'><div class='section-label'>CANONICAL IDENTITIES <span>外部サービスから独立したID</span></div><div class='table-wrap'><table><tr><th>存在 / ENTITY</th><th>SPATIAL ID</th><th>状態</th><th>版</th></tr>");
            for e in &all {
                body.push_str(&format!("<tr><td><b>{}</b><small>EVENT · studio</small></td><td><code>{}</code></td><td><span class='tag'>{}</span></td><td>{}</td></tr>",esc(&e.name),e.id,e.status,e.version))
            }
            body.push_str("</table></div><p class='muted'>このローカル環境ではUUIDv4を使用。Identity Platformの認証・外部ID bindingは未接続です。</p></section>")
        }
        "agent" => {
            body.push_str("<div class='agent-layout'><section class='paper'><div class='eyebrow'>✳ TOOL BROKER</div><h2>次の一手を、いっしょに。</h2><p>操作を選ぶと、Policyが検証します。日時の変更は承認されるまで反映されません。</p><div class='agent-message'>このエリアの探索と、あなたのイベントの日時変更をお手伝いします。</div><a class='outline' href='/?radius=1'>◎ 秋葉原から1km以内を探す</a><h3>イベントの日時を変更する</h3>");
            for e in all.iter() {
                body.push_str(&format!("<details><summary>{}</summary>{}<input type='hidden' name='id' value='{}'><input type='hidden' name='version' value='{}'><label>変更後の日時<input type='datetime-local' name='time' value='{}' required></label><button class='primary'>変更を提案する · R3</button></form></details>",esc(&e.name),start(token,"propose"),e.id,e.version,e.time))
            }
            body.push_str("<p class='muted'>構造化Toolのローカル実行です。生成AIモデルは接続されていません。</p></section><section class='paper'><div class='section-label'>HUMAN APPROVAL <span>承認待ち</span></div>");
            let mut stmt=c.prepare("SELECT a.id,e.name,a.new_time,a.version FROM approvals a JOIN events e ON e.id=a.entity WHERE a.status='PENDING'").unwrap();
            let mut n = 0;
            for row in stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })
                .unwrap()
            {
                let (aid, name, time, version) = row.unwrap();
                n += 1;
                body.push_str(&format!("<article class='approval'><span class='tag'>R3 / 日時変更</span><h3>{}</h3><p>{} → v{}</p><div class='two'>{}<input type='hidden' name='id' value='{aid}'><button class='primary'>承認して反映</button></form>{}<input type='hidden' name='id' value='{aid}'><button class='outline'>却下</button></form></div></article>",esc(&name),esc(&time.replace('T'," ")),version+1,start(token,"approve"),start(token,"reject")))
            }
            if n == 0 {
                body.push_str("<div class='empty'>◎<h3>承認待ちはありません</h3><p>左から変更を提案すると、ここで内容を確認できます。</p></div>")
            }
            body.push_str("<p class='muted'>ローカルでは同じ操作者が提案・承認を体験します。本番の承認者分離は未接続です。R4操作は常に拒否します。</p></section></div>")
        }
        "ops" => {
            let queued: i64 = c
                .query_row(
                    "SELECT count(*) FROM outbox WHERE status='PENDING'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let audits: i64 = c
                .query_row("SELECT count(*) FROM audit", [], |r| r.get(0))
                .unwrap();
            body.push_str(&format!("<div class='metrics'><section class='paper'><span>ローカル位置同期の待機</span><strong>{queued}<small> events</small></strong>{}<button class='outline'>ローカル同期を実行 →</button></form></section><section class='paper'><span>監査イベント</span><strong>{audits}<small> records</small></strong><span class='tag'>HASH CHAIN: {}</span></section><section class='paper'><span>クラウドへの反映</span><strong class='cloud-status'>検証時に実行</strong><p>作成・承認したイベントを保存済みキューから反映します。</p></section></div>",start(token,"sync"),if audit_valid(c){"VALID"}else{"INVALID"}));
            body.push_str(&delivery_panel(c, token));
            body.push_str("<section class='paper'><div class='section-label'>AUDIT TRAIL <span>プロンプト・精密位置を記録しません</span></div><div class='table-wrap'><table><tr><th>日時（UTC）</th><th>操作</th><th>対象</th></tr>");
            let mut stmt = c
                .prepare("SELECT created_at,action,entity FROM audit ORDER BY seq DESC LIMIT 100")
                .unwrap();
            for row in stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .unwrap()
            {
                let (t, a, e) = row.unwrap();
                body.push_str(&format!(
                    "<tr><td>{t}</td><td>{}</td><td><code>{}</code></td></tr>",
                    esc(&a),
                    esc(&e)
                ))
            }
            body.push_str("</table></div><p class='muted'>ハッシュ連鎖は内容の改変検出用です。外部アンカー・保存先の改変防止は未構成です。SLOの達成値は本番計測後に表示します。</p></section>")
        }
        _ => {
            body.push_str(&format!("<form class='search' method='get'><span>⌕</span><input aria-label='イベントや場所を検索' name='q' value='{}' placeholder='場所、イベント、気になることから探す'><select name='radius' aria-label='探索半径'><option value='2'>半径 2 km</option><option value='1' {}>半径 1 km</option><option value='0.5' {}>半径 500 m</option></select><button class='primary'>探索する ↗</button></form><div class='filters'><a class='{}' href='/'>すべて</a>",esc(&v.q),if v.radius=="1"{"selected"}else{""},if v.radius=="0.5"{"selected"}else{""},if v.kind.is_empty(){"selected"}else{""}));
            for k in ["ART", "MUSIC", "WALK", "AR"] {
                body.push_str(&format!(
                    "<a class='{}' href='/?kind={k}'>{k}</a>",
                    if v.kind == k { "selected" } else { "" }
                ))
            }
            body.push_str("<span>基点：秋葉原駅 / 現在地は収集しません</span></div><div class='explorer'><section class='map'><div class='map-heading'><span class='dot'></span> AKIHABARA <small>35.6984° N / 139.7731° E</small></div>");
            body.push_str("<svg viewBox='0 0 800 560' role='img' aria-label='秋葉原周辺のイベント位置。背景は模式図です'><defs><pattern id='grid' width='42' height='42' patternUnits='userSpaceOnUse'><path d='M 42 0 L 0 0 0 42' fill='none' stroke='#2c3837' stroke-width='1'/></pattern></defs><rect width='800' height='560' fill='url(#grid)'/><g fill='#263230' stroke='#364340'><path d='M30 40H165V130H30ZM190 35H330V125H190ZM365 35H465V140H365ZM500 40H625V125H500ZM650 35H770V140H650ZM40 165H190V235H40ZM220 160H360V240H220ZM510 165H650V230H510ZM685 180H775V250H685ZM40 390H190V480H40ZM220 410H350V530H220ZM450 370H600V480H450ZM625 360H755V515H625Z'/></g><path d='M-30 350 C180 280 280 400 440 310 S650 310 850 230' fill='none' stroke='#42605b' stroke-width='35'/><path d='M-30 350 C180 280 280 400 440 310 S650 310 850 230' fill='none' stroke='#71928b' stroke-width='1'/><path d='M-20 110L820 470M380 -20L425 590' stroke='#67736a' stroke-width='14'/><path d='M-20 110L820 470M380 -20L425 590' stroke='#27322e' stroke-width='10' stroke-dasharray='4 7'/><circle cx='400' cy='280' r='155' stroke='#cab990' stroke-dasharray='4 8' fill='none' opacity='.4'/><g fill='#9cac9f' font-size='15'><text x='66' y='85'>御茶ノ水</text><text x='525' y='92'>AKIHABARA</text><text x='120' y='510'>神田</text><text x='620' y='425'>浅草橋</text><text x='440' y='290'>秋葉原駅</text><text x='80' y='322'>神田川</text></g><circle cx='400' cy='280' r='8' fill='#eae4cc'/>");
            for (i, e) in list.iter().enumerate() {
                let x = 400.0 + (e.lon - 139.7731) * 24000.0;
                let y = 280.0 - (e.lat - 35.6984) * 28000.0;
                if (20.0..780.0).contains(&x) && (30.0..540.0).contains(&y) {
                    body.push_str(&format!("<a href='/?selected={}' aria-label='{}'><circle cx='{x}' cy='{y}' r='23' fill='#ed816b' stroke='#202b28' stroke-width='5'/><text x='{x}' y='{}' text-anchor='middle' fill='#172421' font-size='14' font-weight='bold'>{:02}</text></a>",e.id,esc(&e.name),y+5.0,i+1))
                }
            }
            body.push_str("</svg><div class='map-footer'><span>＋ 地点を選んで詳細を見る</span><span>模式図 · 正確な経路案内には使えません</span></div></section><section class='results'><div class='section-label'>NEARBY DISCOVERIES");
            body.push_str(&format!("<span>{:02} 件</span></div>", list.len()));
            if list.is_empty() {
                body.push_str("<div class='empty'><h3>条件に合うイベントがありません</h3><a href='/'>条件をリセット</a></div>")
            }
            for (i, e) in list.iter().enumerate() {
                body.push_str(&format!("<a class='event {}' href='/?selected={}'><div class='event-num'>{:02}</div><div><div class='event-meta'>{} <span> / {:.0} m</span></div><h3>{}</h3><p>{} · {}</p><span class='event-state'>{}</span></div><span>↗</span></a>",if v.selected==e.id{"chosen"}else{""},e.id,i+1,e.kind,distance(e.lat,e.lon)*1000.0,esc(&e.name),esc(&e.place),e.time.replace('T'," "),if e.status=="ACTIVE"{"● 公開中 · サンプル / ローカル"}else{"◌ 位置同期待ち"}))
            }
            body.push_str("</section></div>");
            if let Some(e) = all.iter().find(|e| e.id == v.selected) {
                body.push_str(&format!("<section class='paper detail'><div><span class='tag'>{}</span><h2>{}</h2><p>{} / {} JST</p><code>{}</code></div><div><p>作成者：Nagi Studio<br>関係：CREATED / 版：{}</p><a class='primary' href='/?tab=agent'>Agentで日時を変更 →</a></div></section>",e.kind,esc(&e.name),esc(&e.place),e.time.replace('T'," "),e.id,e.version))
            }
            body.push_str(&format!("<section class='bottom-strip'><div><span class='orbit'>✳</span><div><b>あなたの意図から、街が動き出す。</b><p>Agent Studioで、探索からイベントの変更まで。</p></div></div><a href='/?tab=agent'>Agent Studioを開く ↗</a></section><footer><span>ATLAS / PERSISTENT SPATIAL WEB</span><span>{} identities · Tokyo sandbox · Rust</span></footer>",all.len()));
        }
    }
    body.push_str("</main></div>");
    format!("<!doctype html><html lang='ja'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width, initial-scale=1'><meta name='description' content='街の存在と関係、AgentのアクションをつなぐSpatial Atlas'><title>ATLAS — 街の観測室</title><link rel='stylesheet' href='/style.css'></head><body>{body}</body></html>")
}
