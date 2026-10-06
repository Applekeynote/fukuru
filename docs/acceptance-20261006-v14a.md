# v14a — 地図遷移の回帰修正

v14の本番確認で、専用デモ画面の整理時に地図の呼び出し関数まで欠けていたことを確認。v12へトラフィックを戻し、登録済み705件は保持した。

assets/community.js の mapPage を復元し、既存の SpatialExplore.page と setupMap に接続する。UI・API・データモデル・アクセス権は変更しない。

scripts/map-route-v14-test.cjs は実際の render と setupMap を実行し、地図／glasses の両ルート、選択イベント、アカウント状態、主催・参加予定一覧、近隣一覧の描画・mountを検証する。単なる関数名の存在検査ではない。

ローカルで新しい回帰テスト、discovery-v14-test、JavaScript構文検証に成功。Linuxの共有ライブラリ58件とreleaseビルド、候補URLの地図表示、本番の地図・参加操作は別途配備記録で確認する。
