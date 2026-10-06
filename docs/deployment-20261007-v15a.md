# 本番反映 v15a — 2026-10-07

- 本番URL: https://spatial-community-ybx3gylwba-an.a.run.app/
- GitHub: https://github.com/Applekeynote/fukuru （private / main）
- ソース登録コミット: ae7d93cdc68aba016ec9d905fc6b41594b4195d2
- ソースtree: e3177c5072557718951840f36159d0dbbf452942（126ファイル一致確認）
- Cloud Run: spatial-community / asia-northeast1
- 本番リビジョン: spatial-community-00043-hev（100%）
- 検証タグ: qa-v15a
- イメージ: asia-northeast1-docker.pkg.dev/spatial-atlas-dev-260908-rn/spatial-community/app@sha256:29fa825bc1a6c6881586cd0542679f76fb3e4806d01f2c60046d936dcd38e511
- LinuxバイナリSHA-256: 76c67c6480447e84fd2039ed7b8599d6120ce1f3fecb6489333bdfb2e662eb9c
- ソースアーカイブSHA-256: 80C7120053535745B7DF19FE0F4198F6F672B848BF15EDEFA1E6F3F9DBE31C03
- 切り戻し先: spatial-community-00041-qem

## 確認結果

- Linux: cargo test --lib --locked --offline 62件成功、release build成功。
- /api/health・/api/status: HTTP 200、DB接続を確認。
- 46件のプロフィール: 名前・自己紹介・地域が移行データと一致。
- 本番の人検索: 47人（46地域プロフィールと本人）、佐藤ひなたを検索できる。
- calendar-sync.js・completion.js・completion.css・spatial.js・community.js・offline.js: 配信バイトがソースと一致。
- AI日次制限解除: 2026-10-15 00:00 JST。アプリ機能停止: 2027-01-01 00:00 JST。
- Google Calendar匿名APIアクセスを拒否。本番の予定画面で連携入口を確認。
- 本番AI提案: 愛知県、10月12日11:00〜16:00、予算1,000円、徒歩、写真。無料フォトさんぽと推薦理由を表示。自動参加・予約はしない。
- 広い条件でモデルが空案を返した場合は検証エラーを表示。候補を捏造するフォールバックはない。
- 実位置情報・カメラ映像は取得していない。位置制御は疑似位置の回帰テストで確認。

## 残る本人操作・承認

- Google CalendarのOAuth許可と実カレンダー書込み試験は、自動承認審査が個人カレンダーの権限・書込みとして拒否したため未実施。本人の許可操作に引き継ぐ。
- Cloud Schedulerの課金停止予約は12月1日00:00 JST。12月31日23:59への変更は、12月分の費用が発生し得る延長を自動承認審査が拒否したため、別途承認待ち。補助自動確認の予約日付も変更していない。
- GitHubは非公開。審査側へのGitHub連携許可は提出画面で本人が設定する。