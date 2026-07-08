/* ============================================================
   香农播放器 · i18n 消息目录
   强类型：Messages 为所有文案键的接口，任一语言漏键会在编译期报错。
   当前对外承诺：简体中文与 English（见 data/library.ts 的 LANGUAGES）。
   繁體中文 / 日本語 词条已备好但暂不在界面暴露，待正式承诺后接入。
   专辑、歌手等「内容」不进 i18n，仅界面 chrome 文案走翻译。
   ============================================================ */

export type Locale = "zh-Hans" | "zh-Hant" | "en" | "ja";

/** 所有可翻译文案键。带 {var} 的为插值文案。 */
export interface Messages {
  "nav.albums": string;
  "nav.songs": string;
  "nav.artists": string;
  "nav.favorites": string;
  "nav.settings": string;

  "header.albumSubtitle": string; // {count}
  "placeholder.title": string; // {name}
  "placeholder.body": string;

  "view.grid": string;
  "view.list": string;
  "action.search": string;
  "action.playAlbum": string; // {title}

  "album.kicker": string;
  "album.shufflePlay": string;
  "album.more": string;
  "album.collect": string;
  "album.uncollect": string;
  "album.collected": string;

  "artist.kicker": string;
  "artist.meta": string; // {albums} {songs} {plays}
  "artist.playAll": string;
  "artist.follow": string;
  "artist.unfollow": string;
  "artist.topSongs": string;
  "artist.showAllSongs": string; // {n}
  "artist.showAllAlbums": string; // {n}

  "lyrics.back": string;
  "lyrics.show": string;
  "lyrics.hide": string;
  "lyrics.translation": string;
  "lyrics.glyphTranslation": string;
  "lyrics.romaji": string;
  "lyrics.glyphRomaji": string;
  "lyrics.fontSize": string;
  "lyrics.settings": string;
  "lyrics.none.title": string;
  "lyrics.none.body": string;
  "lyrics.none.search": string;
  "lyrics.none.import": string;
  "lyrics.none.source": string;

  "queue.title": string;
  "queue.clear": string;
  "queue.from": string; // {name}
  "queue.empty": string;

  "theme.light": string;
  "theme.dark": string;
  "theme.system": string;
  "rail.language": string;
  "rail.appearance": string;

  "list.album": string;
  "list.artist": string;
  "list.year": string;
  "list.tracks": string;
  "list.duration": string;

  "unit.tracks": string; // {n}
  "unit.minutes": string; // {n}

  "menu.play": string;
  "menu.playNext": string;
  "menu.addToPlaylist": string;
  "menu.favorite": string;
  "menu.editTags": string;
  "menu.showInfo": string;
  "menu.showLyrics": string;
  "menu.revealInFinder": string;
  "menu.removeFromLibrary": string;

  "player.favorite": string;
  "player.unfavorite": string;
  "player.shuffle": string;
  "player.previous": string;
  "player.play": string;
  "player.pause": string;
  "player.next": string;
  "player.repeat": string;
  "player.queue": string;
  "player.addToPlaylist": string;
  "player.volume": string;

  "window.close": string;
  "window.minimize": string;
  "window.maximize": string;
}

export type MessageKey = keyof Messages;

const zhHans: Messages = {
  "nav.albums": "专辑",
  "nav.songs": "歌曲",
  "nav.artists": "歌手",
  "nav.favorites": "收藏",
  "nav.settings": "设置",

  "header.albumSubtitle": "{count} 张专辑，按最近添加排序",
  "placeholder.title": "「{name}」页",
  "placeholder.body": "核心骨架已就位，此页将在后续迭代中实现",

  "view.grid": "网格",
  "view.list": "列表",
  "action.search": "搜索",
  "action.playAlbum": "播放 {title}",

  "album.kicker": "专辑",
  "album.shufflePlay": "随机播放",
  "album.more": "更多操作",
  "album.collect": "收藏专辑",
  "album.uncollect": "取消收藏专辑",
  "album.collected": "已收藏",

  "artist.kicker": "歌手",
  "artist.meta": "{albums} 张专辑 · {songs} 首歌曲 · 收听 {plays} 次",
  "artist.playAll": "播放全部",
  "artist.follow": "收藏歌手",
  "artist.unfollow": "取消收藏歌手",
  "artist.topSongs": "热门歌曲",
  "artist.showAllSongs": "显示全部 {n} 首",
  "artist.showAllAlbums": "显示全部 {n} 张",

  "lyrics.back": "返回曲库",
  "lyrics.show": "显示歌词",
  "lyrics.hide": "隐藏歌词",
  "lyrics.translation": "翻译",
  "lyrics.glyphTranslation": "译",
  "lyrics.romaji": "音译 / 罗马音",
  "lyrics.glyphRomaji": "音",
  "lyrics.fontSize": "歌词字号",
  "lyrics.settings": "歌词设置（来源 / 偏移校准）",
  "lyrics.none.title": "这首歌还没有歌词",
  "lyrics.none.body": "可以从在线歌词库查找逐字歌词，\n或导入本地 .ttml / .lrc 文件",
  "lyrics.none.search": "在线查找歌词",
  "lyrics.none.import": "导入文件…",
  "lyrics.none.source": "歌词来源：AMLL TTML DB · 也可在歌词设置中调整",

  "queue.title": "继续播放",
  "queue.clear": "清除",
  "queue.from": "来自：{name}",
  "queue.empty": "队列是空的",

  "theme.light": "浅色",
  "theme.dark": "深色",
  "theme.system": "系统",
  "rail.language": "语言",
  "rail.appearance": "切换外观：浅色 / 深色 / 系统",

  "list.album": "专辑",
  "list.artist": "艺人",
  "list.year": "年份",
  "list.tracks": "曲目",
  "list.duration": "时长",

  "unit.tracks": "{n} 首",
  "unit.minutes": "{n} 分钟",

  "menu.play": "播放",
  "menu.playNext": "下一首播放",
  "menu.addToPlaylist": "添加到歌单…",
  "menu.favorite": "收藏",
  "menu.editTags": "编辑标签…",
  "menu.showInfo": "显示专辑简介",
  "menu.showLyrics": "查看歌词",
  "menu.revealInFinder": "在 Finder 中显示",
  "menu.removeFromLibrary": "从曲库中移除",

  "player.favorite": "收藏",
  "player.unfavorite": "取消收藏",
  "player.shuffle": "随机",
  "player.previous": "上一首",
  "player.play": "播放",
  "player.pause": "暂停",
  "player.next": "下一首",
  "player.repeat": "循环",
  "player.queue": "播放队列",
  "player.addToPlaylist": "添加到歌单",
  "player.volume": "音量",

  "window.close": "关闭",
  "window.minimize": "最小化",
  "window.maximize": "最大化",
};

const zhHant: Messages = {
  "nav.albums": "專輯",
  "nav.songs": "歌曲",
  "nav.artists": "歌手",
  "nav.favorites": "收藏",
  "nav.settings": "設定",

  "header.albumSubtitle": "{count} 張專輯，按最近加入排序",
  "placeholder.title": "「{name}」頁",
  "placeholder.body": "核心骨架已就位，此頁將於後續迭代中實作",

  "view.grid": "網格",
  "view.list": "清單",
  "action.search": "搜尋",
  "action.playAlbum": "播放 {title}",

  "album.kicker": "專輯",
  "album.shufflePlay": "隨機播放",
  "album.more": "更多操作",
  "album.collect": "收藏專輯",
  "album.uncollect": "取消收藏專輯",
  "album.collected": "已收藏",

  "artist.kicker": "歌手",
  "artist.meta": "{albums} 張專輯 · {songs} 首歌曲 · 聆聽 {plays} 次",
  "artist.playAll": "播放全部",
  "artist.follow": "收藏歌手",
  "artist.unfollow": "取消收藏歌手",
  "artist.topSongs": "熱門歌曲",
  "artist.showAllSongs": "顯示全部 {n} 首",
  "artist.showAllAlbums": "顯示全部 {n} 張",

  "lyrics.back": "返回曲庫",
  "lyrics.show": "顯示歌詞",
  "lyrics.hide": "隱藏歌詞",
  "lyrics.translation": "翻譯",
  "lyrics.glyphTranslation": "譯",
  "lyrics.romaji": "音譯 / 羅馬音",
  "lyrics.glyphRomaji": "音",
  "lyrics.fontSize": "歌詞字號",
  "lyrics.settings": "歌詞設定（來源 / 偏移校準）",
  "lyrics.none.title": "這首歌還沒有歌詞",
  "lyrics.none.body": "可以從線上歌詞庫查找逐字歌詞，\n或匯入本地 .ttml / .lrc 檔案",
  "lyrics.none.search": "線上查找歌詞",
  "lyrics.none.import": "匯入檔案…",
  "lyrics.none.source": "歌詞來源：AMLL TTML DB · 也可在歌詞設定中調整",

  "queue.title": "繼續播放",
  "queue.clear": "清除",
  "queue.from": "來自：{name}",
  "queue.empty": "佇列是空的",

  "theme.light": "淺色",
  "theme.dark": "深色",
  "theme.system": "系統",
  "rail.language": "語言",
  "rail.appearance": "切換外觀：淺色 / 深色 / 系統",

  "list.album": "專輯",
  "list.artist": "演出者",
  "list.year": "年份",
  "list.tracks": "曲目",
  "list.duration": "時長",

  "unit.tracks": "{n} 首",
  "unit.minutes": "{n} 分鐘",

  "menu.play": "播放",
  "menu.playNext": "下一首播放",
  "menu.addToPlaylist": "加入播放清單…",
  "menu.favorite": "收藏",
  "menu.editTags": "編輯標籤…",
  "menu.showInfo": "顯示專輯簡介",
  "menu.showLyrics": "查看歌詞",
  "menu.revealInFinder": "在 Finder 中顯示",
  "menu.removeFromLibrary": "從曲庫中移除",

  "player.favorite": "收藏",
  "player.unfavorite": "取消收藏",
  "player.shuffle": "隨機",
  "player.previous": "上一首",
  "player.play": "播放",
  "player.pause": "暫停",
  "player.next": "下一首",
  "player.repeat": "循環",
  "player.queue": "播放佇列",
  "player.addToPlaylist": "加入播放清單",
  "player.volume": "音量",

  "window.close": "關閉",
  "window.minimize": "最小化",
  "window.maximize": "最大化",
};

const en: Messages = {
  "nav.albums": "Albums",
  "nav.songs": "Songs",
  "nav.artists": "Artists",
  "nav.favorites": "Favorites",
  "nav.settings": "Settings",

  "header.albumSubtitle": "{count} albums, sorted by recently added",
  "placeholder.title": "{name}",
  "placeholder.body": "Core scaffold is in place; this page ships in a later iteration",

  "view.grid": "Grid",
  "view.list": "List",
  "action.search": "Search",
  "action.playAlbum": "Play {title}",

  "album.kicker": "Album",
  "album.shufflePlay": "Shuffle",
  "album.more": "More",
  "album.collect": "Favorite album",
  "album.uncollect": "Unfavorite album",
  "album.collected": "Favorited",

  "artist.kicker": "Artist",
  "artist.meta": "{albums} albums · {songs} songs · {plays} plays",
  "artist.playAll": "Play All",
  "artist.follow": "Favorite artist",
  "artist.unfollow": "Unfavorite artist",
  "artist.topSongs": "Top Songs",
  "artist.showAllSongs": "Show all {n}",
  "artist.showAllAlbums": "Show all {n}",

  "lyrics.back": "Back to Library",
  "lyrics.show": "Show lyrics",
  "lyrics.hide": "Hide lyrics",
  "lyrics.translation": "Translation",
  "lyrics.glyphTranslation": "译",
  "lyrics.romaji": "Transliteration",
  "lyrics.glyphRomaji": "音",
  "lyrics.fontSize": "Lyrics size",
  "lyrics.settings": "Lyrics settings (source / offset)",
  "lyrics.none.title": "No lyrics for this song yet",
  "lyrics.none.body": "Search word-by-word lyrics online,\nor import a local .ttml / .lrc file",
  "lyrics.none.search": "Search Lyrics Online",
  "lyrics.none.import": "Import File…",
  "lyrics.none.source": "Lyrics source: AMLL TTML DB · adjustable in lyrics settings",

  "queue.title": "Up Next",
  "queue.clear": "Clear",
  "queue.from": "From: {name}",
  "queue.empty": "The queue is empty",

  "theme.light": "Light",
  "theme.dark": "Dark",
  "theme.system": "System",
  "rail.language": "Language",
  "rail.appearance": "Switch appearance: Light / Dark / System",

  "list.album": "Album",
  "list.artist": "Artist",
  "list.year": "Year",
  "list.tracks": "Tracks",
  "list.duration": "Duration",

  "unit.tracks": "{n} tracks",
  "unit.minutes": "{n} min",

  "menu.play": "Play",
  "menu.playNext": "Play Next",
  "menu.addToPlaylist": "Add to Playlist…",
  "menu.favorite": "Favorite",
  "menu.editTags": "Edit Tags…",
  "menu.showInfo": "Show Album Info",
  "menu.showLyrics": "Show Lyrics",
  "menu.revealInFinder": "Reveal in Finder",
  "menu.removeFromLibrary": "Remove from Library",

  "player.favorite": "Favorite",
  "player.unfavorite": "Unfavorite",
  "player.shuffle": "Shuffle",
  "player.previous": "Previous",
  "player.play": "Play",
  "player.pause": "Pause",
  "player.next": "Next",
  "player.repeat": "Repeat",
  "player.queue": "Queue",
  "player.addToPlaylist": "Add to Playlist",
  "player.volume": "Volume",

  "window.close": "Close",
  "window.minimize": "Minimize",
  "window.maximize": "Maximize",
};

const ja: Messages = {
  "nav.albums": "アルバム",
  "nav.songs": "曲",
  "nav.artists": "アーティスト",
  "nav.favorites": "お気に入り",
  "nav.settings": "設定",

  "header.albumSubtitle": "{count} 枚のアルバム・追加日順",
  "placeholder.title": "「{name}」ページ",
  "placeholder.body": "コア構造は完成済み。このページは今後の反復で実装します",

  "view.grid": "グリッド",
  "view.list": "リスト",
  "action.search": "検索",
  "action.playAlbum": "{title} を再生",

  "album.kicker": "アルバム",
  "album.shufflePlay": "シャッフル再生",
  "album.more": "その他",
  "album.collect": "アルバムをお気に入りに追加",
  "album.uncollect": "アルバムをお気に入りから削除",
  "album.collected": "お気に入り済み",

  "artist.kicker": "アーティスト",
  "artist.meta": "{albums} 枚のアルバム · {songs} 曲 · 再生 {plays} 回",
  "artist.playAll": "すべて再生",
  "artist.follow": "アーティストをお気に入りに追加",
  "artist.unfollow": "アーティストをお気に入りから削除",
  "artist.topSongs": "人気曲",
  "artist.showAllSongs": "全 {n} 曲を表示",
  "artist.showAllAlbums": "全 {n} 枚を表示",

  "lyrics.back": "ライブラリに戻る",
  "lyrics.show": "歌詞を表示",
  "lyrics.hide": "歌詞を隠す",
  "lyrics.translation": "翻訳",
  "lyrics.glyphTranslation": "訳",
  "lyrics.romaji": "音訳 / ローマ字",
  "lyrics.glyphRomaji": "音",
  "lyrics.fontSize": "歌詞サイズ",
  "lyrics.settings": "歌詞設定（ソース / オフセット）",
  "lyrics.none.title": "この曲にはまだ歌詞がありません",
  "lyrics.none.body": "オンライン歌詞ライブラリで検索するか、\nローカルの .ttml / .lrc ファイルを読み込めます",
  "lyrics.none.search": "オンラインで歌詞を探す",
  "lyrics.none.import": "ファイルを読み込む…",
  "lyrics.none.source": "歌詞ソース：AMLL TTML DB · 歌詞設定で調整可能",

  "queue.title": "次に再生",
  "queue.clear": "クリア",
  "queue.from": "出典：{name}",
  "queue.empty": "キューは空です",

  "theme.light": "ライト",
  "theme.dark": "ダーク",
  "theme.system": "システム",
  "rail.language": "言語",
  "rail.appearance": "外観を切替：ライト / ダーク / システム",

  "list.album": "アルバム",
  "list.artist": "アーティスト",
  "list.year": "年",
  "list.tracks": "曲数",
  "list.duration": "長さ",

  "unit.tracks": "{n} 曲",
  "unit.minutes": "{n} 分",

  "menu.play": "再生",
  "menu.playNext": "次に再生",
  "menu.addToPlaylist": "プレイリストに追加…",
  "menu.favorite": "お気に入り",
  "menu.editTags": "タグを編集…",
  "menu.showInfo": "アルバム情報を表示",
  "menu.showLyrics": "歌詞を表示",
  "menu.revealInFinder": "Finder で表示",
  "menu.removeFromLibrary": "ライブラリから削除",

  "player.favorite": "お気に入り",
  "player.unfavorite": "お気に入り解除",
  "player.shuffle": "シャッフル",
  "player.previous": "前へ",
  "player.play": "再生",
  "player.pause": "一時停止",
  "player.next": "次へ",
  "player.repeat": "リピート",
  "player.queue": "再生キュー",
  "player.addToPlaylist": "プレイリストに追加",
  "player.volume": "音量",

  "window.close": "閉じる",
  "window.minimize": "最小化",
  "window.maximize": "最大化",
};

export const CATALOG: Record<Locale, Messages> = {
  "zh-Hans": zhHans,
  "zh-Hant": zhHant,
  en,
  ja,
};
