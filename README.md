# ふくる

地域の体験を探し、準備して、現地へ向かうPWAです。フクロウのAIキャラクター「ぽよ」が服装・持ち物や行程を提案します。

**アプリ**: https://spatial-community-ybx3gylwba-an.a.run.app/

## 機能
- 日時・地域・料金・興味によるイベント検索、Watch、参加予定
- Google Maps / Street View / スマートフォンの2D AR案内
- 移動状態と位置精度に応じたナビ表示、サーバーで検証する報酬処理
- 開催地の天気、AIによる服装・準備・行程の提案
- トーク、メンション、通知、Web Push、プロフィール
- 予定のGoogleカレンダーへの一方向反映（ふくる専用カレンダー・本人のOAuth許可が必要）

## 構成
Rust / Axum、HTML / CSS / JavaScript、Service Worker。Cloud Run、Spanner、Vertex AI / Gemini、Google Maps / Routes / Geocoding、Open-Meteoを利用します。バックアップはCloud Scheduler / Cloud Run Jobs / Cloud Storage、監視はCloud Monitoringです。

## ローカル起動
Rust 1.85以上とCargoが必要です。ローカルではSQLiteを利用し、公開サービスのデータや実アカウントは読み込みません。

```sh
cargo test --lib --locked
cargo build --bin spatial_community --locked
cargo run --bin spatial_community --locked
```

http://127.0.0.1:8790/ を開いてください。クラウドAPI、Googleログイン・Calendar・Maps、AIは環境設定と各APIの利用許可が必要です。認証情報はこのリポジトリに含めていません。

JavaScript検証では `scripts/navigation-tools.package.json` の依存を `target/navigation-tools/node_modules` に配置します。主要な検証は `scripts/*-test.cjs`、Rust側は各モジュールのテストです。

## 主なファイル
- `src/community/`: 認証、権限、イベント、通知、地図、ナビ、AI、Calendar API
- `assets/`: 画面、デザイン、位置情報、AR描画、Service Worker
- `data/`: 操作確認用の地域イベント・疑似プロフィール
- `scripts/`: 検証、データ生成、ソースアーカイブ作成
- `infra/`: コンテナ・DB・バックアップ設定
- `docs/`: 検証記録と設計

## ソースの対応
本番反映対象はv16（2026-10-07）。承認済みアーカイブのSHA-256は `7B170E2DF25F6DD819E6AFFCC8CE130A47B39BB9FF9BFCE4EB594995D3EE53C3` です。このリポジトリにはアーカイブのソースに、README・.gitignoreと本番反映記録を加えています。DB、ログ、認証情報、実アカウントの画像は含みません。

## 提供期間
2026-10-15 00:00 JSTからAIの日次回数制限を解除する実装です。2027-01-01 00:00 JSTからアプリ機能を停止します。クラウドの課金停止予約は別設定で、変更完了までは `docs` の運用記録を確認してください。
