# Navigation v10 — 実装と運用境界

## 既存構成

既存のJavaScript SPA、Axum、SQLite／SpannerのRecordsトランザクションを維持する。新しいフロントエンドフレームワーク、地図プロバイダー、ルーター、外部ウォレットは追加していない。Google Maps JavaScript APIのRoutesライブラリを使い、徒歩・自転車・車の経路を正規化する。

|責務|実装|
|---|---|
|経路・速度・停止・到着・危険箇所・信頼度|assets/navigation.js|
|Web／カメラ描画、視認性、短いフィードバック|assets/navigation-renderer.js|
|地図・Street View・位置取得・カメラの接続|assets/spatial.js、routes.js、location.js|
|報酬API境界と再試行|assets/navigation-rewards.js|
|共通デザイン|assets/navigation.css|
|報酬・安全配置・移動整合性・残高|src/community/navigation.rs|
|認証・CSRF・閲覧権限・頻度制限|src/community/server_navigation.inc.rs|

## 状態と描画

PLANNING → 開始 → WALKING／CYCLING／DRIVING。新鮮なGPSと10秒の停止判定でSTOPPED。曲がりまで50mでDECISION_POINT、20m／8mでゲートを強める。既知の危険点の25m圏とGPS誤差を考慮してHAZARD_ZONEへ。15m以内・速度0.45m/s以下・精度15m以内・連続3測位でARRIVED。

速度はGPS speedと位置差分の大きい方で判定する。3.2m/s以上で自転車、9m/s以上で車として表示量を減らす。車経路は速度0でもカメラを許可しない。経路なしのARでも速度を監視する。状態はカメラのDOMやGoogle Mapsインスタンスから独立している。

Street ViewはPLANNING／STOPPEDだけで、本人がプレビューを開いた時に表示する。移動中は1ペイン。Web経路は手前300mを強く、先を薄くする。カメラ上の連続線は描かない。方向マーカー2〜3個、交差点ゲート、画面端の方位を使う。危険時は報酬・POI・キャラクター・一部操作を非表示。自転車では同様に情報を減らし、車ではカメラ自体を停止する。

通常のブラウザGPS＋コンパスはMEDIUMであり、方向マーカーは画面相対の表示である。HIGHは検証済みのposeとprojectPointを供給するアダプター専用。地理座標だけを正確な世界アンカーとして扱わない。projectPointは正規化画面座標{x,y,visible}を返し、投影不能ならnullを返す。LOW／古いGPSでは空間オブジェクトを隠す。ネイティブAR／グラスの具体的なposeアダプターは未搭載。

## API

- POST /api/navigation/session：destination、mode、path。本人確認、CSRF、イベント閲覧権限を確認する。
- POST /api/navigation/evidence：route_session_id、nonce、sequence、request_id、position、imu、device_integrity。
- position：lat、lon、accuracy、timestamp、speed、heading。IMU／端末認証は将来用の境界であり、現状のブラウザ申告値を認証済みセンサーとは扱わない。
- 返却値：nonce、sequence、status、risk_score、collected、reward_count。画面はサーバーの確定値のみ反映する。

セッションは2時間、単一ユーザーに紐付く。新規セッションで以前のセッションを閉じる。nonceはサーバーでハッシュ保管し、受理ごとに更新する。直前の同一リクエストだけ結果を再返却し、異なる内容・古いnonce・不正な順序を拒否する。端末は未送信の古い測位を溜めず、再試行中の証跡だけメモリーで保持する。オフライン取得・後日の再送・クライアントでの加算は行わない。

## 安全配置と報酬

navigation_corridorは通常のアプリ操作から書き込めない運用データ。reviewed_by、expires_at、classification、safety_zone、points、hazards、allowed_modesを使う。歩道確認済み・期限内・危険情報なし・安全領域内・推奨経路から5m以内だけを採用する。未指定のallowed_modesは徒歩だけ。道路・私有地・不明な区域へ自動生成しない。危険点近くのコインは除外する。全世界の固定コイン一覧を端末へ配布しない。

コインはセッションごとにUUIDを生成し、collection_ruleとstatusを持つ。GPSの誤差円が安全領域内に収まることも確認する。取得はボタン操作や厳密接触ではなく正常通過の証跡から自動判定する。半径4mを基準に精度を考慮するが、安全領域判定は緩めない。報酬対象コインがないセッションではGNSS証跡を送信しない。デモは報酬対象外。

Route Coin／Discovery Token／Journey Bonusの型を用意する。Route Coinと安全確認済み到着時のJourney Bonusを実装し、Discovery Tokenの実際の付与条件は未実装。残高は譲渡・換金APIを持たないソフト通貨。アイテム購入や装飾テーマのショップは未実装。

主なConfig初期値：2時間、8セッション/日、取得半径4m、最大GPS誤差12m、配置間隔100m、区間24時間クールダウン、日次100、50取得以降は逓減、到着滞在10秒。リスク境界21／51／81。即BANは行わずverification／pending／rejectedを返す。時計差、証跡順序、瞬間移動、移動手段に合わない速度、逆行、経路からの逸脱を判定する。

Configはnavigation_config/runtimeから読み、境界値を検証する。公開APIにConfigや安全領域の編集権限を追加していない。運用責任者が確認済みデータを登録するまで本番のコイン配置は空になる。実際の歩道情報や危険情報を推測で補完しない。

## 保持・権限・限界

経路・直近の正確な位置は期限切れ後に次のナビ操作または日次保守で除去する。集計台帳は正確な位置を持たず、7日・直近32セッション/人。残高は本人だけのデータとしてエクスポート／削除に含める。nonceや再送用レスポンスはエクスポートしない。privacy.htmlの既存の説明も実装に合わせて修正した。バックアップは既存の保持期間に従う。

GPS偽装、端末改変、エミュレーター、脱獄、複数アカウントをWebだけで確実に判別する機能はない。端末認証・センサー検証・運用審査との接続が必要。リスク判定を完全な偽装防止と表現しない。Google経路のクライアント申告は幾何を検証するが、サーバーが再計算した署名済み経路ではない。独立した安全領域データを報酬の必須条件とする。

既存Recordsストアの容量制限を維持し、証跡を1測位1レコードで無制限保存しない。利用者が大幅に増える場合、保持上限の運用監視と索引を持つストアアダプターが必要。

## 再現検証

開発ツールだけをtarget/navigation-toolsへインストールする。scripts/navigation-tools.package.jsonをそのディレクトリのpackage.jsonにコピーし、通常のパッケージレジストリから依存を導入する。アプリの本番ランタイムにはこれらのNode依存は不要。

- cargo test --lib --locked --offline
- cargo build --bin spatial_community --locked --offline
- cargo build --release --bin spatial_community --locked --offline
- cargo clippy --lib --bin spatial_community --locked --offline
- node scripts/navigation-test.cjs
- node scripts/navigation-dom-test.cjs
- node scripts/navigation-api-test.cjs
- node target/navigation-tools/node_modules/eslint/bin/eslint.js --config scripts/navigation-eslint.config.mjs assets/navigation.js assets/navigation-renderer.js assets/navigation-rewards.js assets/spatial.js assets/routes.js assets/location.js
- node target/navigation-tools/node_modules/typescript/bin/tsc --noEmit --allowJs --checkJs --target es2022 --lib es2022,dom assets/navigation-types.d.ts assets/navigation.js assets/navigation-renderer.js assets/navigation-rewards.js

navigation-browser-harness.cjsは127.0.0.1:8808専用の検証入力。GPS、経路、カメラ背景を模擬する。本番HTMLは読み込まず、Cloud Runで公開しない。製品の状態制御・Google連携・報酬APIをモック実装で置き換えたものではない。画面キャプチャの灰色の背景は実カメラではない。
