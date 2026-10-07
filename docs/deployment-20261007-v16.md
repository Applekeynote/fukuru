# 本番反映 v16 — 2026-10-07

- 本番URL: https://spatial-community-ybx3gylwba-an.a.run.app/
- Cloud Run: spatial-community / asia-northeast1
- リビジョン: spatial-community-00045-heq（本番100%）
- イメージ: asia-northeast1-docker.pkg.dev/spatial-atlas-dev-260908-rn/spatial-community/app@sha256:506f727c07d3976bc06227ccc43f84bdbd0bea77fb6b5a1d19b8f7d59cd011f4
- LinuxバイナリSHA-256: c65abe6f5a863700328f5f915553cb0fdf756b7c8f50a5e7e6df61c0f5862a46
- 承認済みソースSHA-256: 7B170E2DF25F6DD819E6AFFCC8CE130A47B39BB9FF9BFCE4EB594995D3EE53C3
- ソースコミット: a070f57e124b3aafbd37e79ce27017ffc1359157
- 切り戻し先: spatial-community-00043-hev

## OAuthの変更

Google Auth Platformでデータアクセスを calendar.app.created と openid に絞って保存し、公開ステータスを「本番環境」に変更。機密・制限付きスコープはない。テストユーザー登録を必要とする制限を解除した。

連携は本人のGoogle許可と反映操作後に「ふくる」専用カレンダーを作成・更新する。メインカレンダーや既存予定一覧は取得しない。以前メインカレンダーへ追加した予定を移行・削除する処理も行わない。Googleアカウントの組織ポリシーは適用される。

## 確認結果

- Windows・Linux: Rust 63件すべて成功。Linux release build成功（終了コード0）。
- JavaScript構文・Calendar v15/v16・Lifecycle回帰検証成功。Clippyの重大エラーなし。
- 検証用URLと本番URLで /api/health・/api/status がHTTP 200。
- calendar-sync.js / community.js / completion.js / spatial.js / offline.js の配信バイトがソースと一致。
- 未ログインの /api/google-calendar-link（POST）は403、/api/google-calendar-plan（GET）は400で拒否。
- 検証スクリプトがplanへPOSTして405になった箇所は、GETへ修正して再検証成功。アプリのAPI変更は不要だった。
- 本番予定画面から新しい連携ダイアログを開き、15件の参加予定と専用カレンダーへの連携入口を確認。
- Googleの個人アカウントでの許可・実カレンダーへの書込みは本人操作へ引継ぎ。実予定の反映成功はまだ確認していない。

## 記録の扱い

ソースアーカイブは承認時の内容を保持している。本書、README、infra/project.jsonの本番メタデータは配備後の追記としてGitHubへ登録する。

Cloud Schedulerの課金停止予約の日付変更は別の承認待ちで、今回変更していない。
