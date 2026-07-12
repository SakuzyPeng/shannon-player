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
  "nav.search": string;
  "nav.favorites": string;
  "nav.settings": string;

  "header.albumSubtitle": string; // {count}
  "placeholder.title": string; // {name}
  "placeholder.body": string;

  "search.placeholder": string;
  "search.scopeAll": string;
  "search.recent": string;
  "search.emptyTitle": string; // {q}
  "search.emptyBody": string;
  "search.clear": string;

  "settings.secLibrary": string;
  "settings.secPlayback": string;
  "settings.secLyrics": string;
  "settings.secAppearance": string;
  "settings.secAbout": string;
  "settings.musicFolders": string;
  "settings.addFolder": string;
  "settings.removeFolder": string;
  "settings.folderTracks": string; // {n}
  "settings.statusScanned": string;
  "settings.statusWatching": string;
  "settings.watch": string;
  "settings.watchDesc": string;
  "settings.cloud": string;
  "settings.cloudDesc": string;
  "settings.loudness": string;
  "settings.loudnessDesc": string;
  "settings.onlineLyrics": string;
  "settings.onlineLyricsDesc": string;
  "settings.wordByWord": string;
  "settings.wordByWordDesc": string;
  "settings.appearance": string;
  "settings.appearanceDesc": string;
  "settings.language": string;
  "settings.languageDesc": string;
  "settings.appName": string;
  "settings.aboutTagline": string;
  "settings.sourceCode": string;
  "settings.backers": string;
  "settings.donate": string;

  "firstRun.welcomeTitle": string;
  "firstRun.welcomeBody": string;
  "firstRun.addFolder": string;
  "firstRun.formats": string;
  "firstRun.scanningTitle": string;
  "firstRun.foundLabel": string;
  "firstRun.foundUnit": string; // {m}
  "firstRun.background": string;
  "firstRun.cancel": string;
  "firstRun.doneTitle": string;
  "firstRun.doneBody": string; // {n} {m} {a}
  "firstRun.startListening": string;

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
  "artists.subtitle": string; // {n}
  "artists.filterPlaceholder": string;
  "artists.emptyTitle": string; // {q}
  "artists.emptyBody": string;

  "songs.subtitle": string; // {n} {h} {m}
  "songs.sortMenu": string;
  "songs.sortByTitle": string;
  "songs.sortByArtist": string;
  "songs.sortRecent": string;
  "songs.filterPlaceholder": string;
  "songs.filterClear": string;
  "songs.colArtist": string;
  "songs.groupMeta": string; // {albums} {n}
  "songs.emptyTitle": string; // {q}
  "songs.emptyBody": string;

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
  "menu.removeFromPlaylist": string;

  "favorites.subtitle": string; // {songs} {albums} {artists} {playlists}
  "favorites.playlists": string;
  "favorites.filterPlaceholder": string;
  "favorites.sortRecent": string;
  "favorites.artistMeta": string; // {n}
  "favorites.emptyHint": string;
  "favorites.emptyFilter": string; // {q}
  "favorites.emptySongs": string;
  "favorites.emptyAlbums": string;
  "favorites.emptyArtists": string;
  "favorites.emptyPlaylists": string;
  "playlist.kicker": string;
  "playlist.meta": string; // {n} {m} {updated}
  "playlist.filterPlaceholder": string;
  "playlist.dragToReorder": string;
  "playlist.emptyTitle": string; // {q}
  "playlist.emptyTryGlobal": string;
  "playlist.emptyGlobalSearch": string;

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
  "nav.search": "搜索",
  "nav.favorites": "收藏",
  "nav.settings": "设置",

  "header.albumSubtitle": "{count} 张专辑，按最近添加排序",
  "placeholder.title": "「{name}」页",
  "placeholder.body": "核心骨架已就位，此页将在后续迭代中实现",

  "search.placeholder": "搜索歌曲、专辑、歌手、歌单…",
  "search.scopeAll": "全部",
  "search.recent": "最近搜索",
  "search.emptyTitle": "没有找到“{q}”",
  "search.emptyBody": "检查拼写，或试试歌手名、专辑名的其他写法",
  "search.clear": "清空",

  "settings.secLibrary": "曲库",
  "settings.secPlayback": "播放",
  "settings.secLyrics": "歌词",
  "settings.secAppearance": "外观与语言",
  "settings.secAbout": "关于",
  "settings.musicFolders": "音乐文件夹",
  "settings.addFolder": "添加文件夹…",
  "settings.removeFolder": "移除文件夹",
  "settings.folderTracks": "{n} 首",
  "settings.statusScanned": "已扫描",
  "settings.statusWatching": "监听中",
  "settings.watch": "实时监听文件变化",
  "settings.watchDesc": "文件夹内容变动时自动更新曲库",
  "settings.cloud": "云盘占位符检测",
  "settings.cloudDesc": "跳过尚未下载到本地的占位文件，避免触发云端下载",
  "settings.loudness": "响度均一化",
  "settings.loudnessDesc": "仅调整整曲音量增益，不压缩动态范围",
  "settings.onlineLyrics": "在线歌词源",
  "settings.onlineLyricsDesc": "从 AMLL TTML DB 获取逐字歌词，本地歌词优先",
  "settings.wordByWord": "逐字高亮",
  "settings.wordByWordDesc": "关闭后按行高亮，降低功耗",
  "settings.appearance": "外观",
  "settings.appearanceDesc": "跟随系统时将自动在日出日落间切换",
  "settings.language": "界面语言",
  "settings.languageDesc": "重启后对全部界面文案生效",
  "settings.appName": "香农播放器",
  "settings.aboutTagline": "永久免费，无付费功能",
  "settings.sourceCode": "源代码",
  "settings.backers": "支持者名单",
  "settings.donate": "打赏支持",

  "firstRun.welcomeTitle": "欢迎使用香农播放器",
  "firstRun.welcomeBody": "你的音乐留在本地，播放器只是帮你听见它。\n先告诉我音乐放在哪里。",
  "firstRun.addFolder": "添加音乐文件夹",
  "firstRun.formats": "支持 FLAC · MP3 · AAC · OGG · WAV 等格式，也可以直接把文件夹拖进来",
  "firstRun.scanningTitle": "正在整理你的曲库",
  "firstRun.foundLabel": "已找到",
  "firstRun.foundUnit": "首歌曲 · {m} 张专辑",
  "firstRun.background": "在后台继续",
  "firstRun.cancel": "取消",
  "firstRun.doneTitle": "曲库整理好了",
  "firstRun.doneBody": "{n} 首歌曲 · {m} 张专辑 · {a} 位歌手\n之后新增的文件会自动加入，无需再次扫描",
  "firstRun.startListening": "开始听歌",

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
  "artists.subtitle": "{n} 位歌手",
  "artists.filterPlaceholder": "过滤歌手…",
  "artists.emptyTitle": "没有找到“{q}”",
  "artists.emptyBody": "检查拼写，或试试其他歌手名",

  "songs.subtitle": "{n} 首 · {h} 小时 {m} 分钟",
  "songs.sortMenu": "排序方式",
  "songs.sortByTitle": "按标题",
  "songs.sortByArtist": "按歌手",
  "songs.sortRecent": "最近添加",
  "songs.filterPlaceholder": "过滤全部歌曲…",
  "songs.filterClear": "清空并关闭",
  "songs.colArtist": "歌手",
  "songs.groupMeta": "{albums} 张专辑 · {n} 首",
  "songs.emptyTitle": "没有找到“{q}”",
  "songs.emptyBody": "检查拼写，或试试歌手名、专辑名",

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
  "menu.removeFromPlaylist": "从歌单中移除",

  "favorites.subtitle": "{songs} 首歌曲 · {albums} 张专辑 · {artists} 位歌手 · {playlists} 歌单",
  "favorites.playlists": "歌单",
  "favorites.filterPlaceholder": "过滤收藏…",
  "favorites.sortRecent": "最近收藏",
  "favorites.artistMeta": "{n} 张专辑",
  "favorites.emptyHint": "在任意页面 hover 封面或曲目行，点击爱心即可收藏到这里",
  "favorites.emptyFilter": "没有找到“{q}”",
  "favorites.emptySongs": "还没有收藏歌曲",
  "favorites.emptyAlbums": "还没有收藏专辑",
  "favorites.emptyArtists": "还没有收藏歌手",
  "favorites.emptyPlaylists": "还没有收藏歌单",
  "playlist.kicker": "播放列表",
  "playlist.meta": "{n} 首 · {m} 分钟 · {updated}",
  "playlist.filterPlaceholder": "过滤歌单内歌曲…",
  "playlist.dragToReorder": "拖拽排序",
  "playlist.emptyTitle": "歌单里没有“{q}”",
  "playlist.emptyTryGlobal": "试试 ",
  "playlist.emptyGlobalSearch": "在整个曲库中搜索",

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
  "nav.search": "搜尋",
  "nav.favorites": "收藏",
  "nav.settings": "設定",

  "header.albumSubtitle": "{count} 張專輯，按最近加入排序",
  "placeholder.title": "「{name}」頁",
  "placeholder.body": "核心骨架已就位，此頁將於後續迭代中實作",

  "search.placeholder": "搜尋歌曲、專輯、歌手、歌單…",
  "search.scopeAll": "全部",
  "search.recent": "最近搜尋",
  "search.emptyTitle": "沒有找到「{q}」",
  "search.emptyBody": "檢查拼寫，或試試歌手名、專輯名的其他寫法",
  "search.clear": "清空",

  "settings.secLibrary": "曲庫",
  "settings.secPlayback": "播放",
  "settings.secLyrics": "歌詞",
  "settings.secAppearance": "外觀與語言",
  "settings.secAbout": "關於",
  "settings.musicFolders": "音樂資料夾",
  "settings.addFolder": "新增資料夾…",
  "settings.removeFolder": "移除資料夾",
  "settings.folderTracks": "{n} 首",
  "settings.statusScanned": "已掃描",
  "settings.statusWatching": "監聽中",
  "settings.watch": "即時監聽檔案變化",
  "settings.watchDesc": "資料夾內容變動時自動更新曲庫",
  "settings.cloud": "雲端佔位符偵測",
  "settings.cloudDesc": "略過尚未下載到本機的佔位檔，避免觸發雲端下載",
  "settings.loudness": "響度均一化",
  "settings.loudnessDesc": "僅調整整曲音量增益，不壓縮動態範圍",
  "settings.onlineLyrics": "線上歌詞源",
  "settings.onlineLyricsDesc": "從 AMLL TTML DB 取得逐字歌詞，本機歌詞優先",
  "settings.wordByWord": "逐字高亮",
  "settings.wordByWordDesc": "關閉後按行高亮，降低功耗",
  "settings.appearance": "外觀",
  "settings.appearanceDesc": "跟隨系統時將自動在日出日落間切換",
  "settings.language": "介面語言",
  "settings.languageDesc": "重啟後對全部介面文案生效",
  "settings.appName": "香農播放器",
  "settings.aboutTagline": "永久免費，無付費功能",
  "settings.sourceCode": "原始碼",
  "settings.backers": "支持者名單",
  "settings.donate": "抖內支持",

  "firstRun.welcomeTitle": "歡迎使用香農播放器",
  "firstRun.welcomeBody": "你的音樂留在本機，播放器只是幫你聽見它。\n先告訴我音樂放在哪裡。",
  "firstRun.addFolder": "新增音樂資料夾",
  "firstRun.formats": "支援 FLAC · MP3 · AAC · OGG · WAV 等格式，也可以直接把資料夾拖進來",
  "firstRun.scanningTitle": "正在整理你的曲庫",
  "firstRun.foundLabel": "已找到",
  "firstRun.foundUnit": "首歌曲 · {m} 張專輯",
  "firstRun.background": "在背景繼續",
  "firstRun.cancel": "取消",
  "firstRun.doneTitle": "曲庫整理好了",
  "firstRun.doneBody": "{n} 首歌曲 · {m} 張專輯 · {a} 位歌手\n之後新增的檔案會自動加入，無需再次掃描",
  "firstRun.startListening": "開始聽歌",

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
  "artists.subtitle": "{n} 位歌手",
  "artists.filterPlaceholder": "過濾歌手…",
  "artists.emptyTitle": "沒有找到「{q}」",
  "artists.emptyBody": "檢查拼寫，或試試其他歌手名",

  "songs.subtitle": "{n} 首 · {h} 小時 {m} 分鐘",
  "songs.sortMenu": "排序方式",
  "songs.sortByTitle": "按標題",
  "songs.sortByArtist": "按歌手",
  "songs.sortRecent": "最近加入",
  "songs.filterPlaceholder": "過濾全部歌曲…",
  "songs.filterClear": "清空並關閉",
  "songs.colArtist": "歌手",
  "songs.groupMeta": "{albums} 張專輯 · {n} 首",
  "songs.emptyTitle": "沒有找到「{q}」",
  "songs.emptyBody": "檢查拼寫，或試試歌手名、專輯名",

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
  "menu.removeFromPlaylist": "從歌單中移除",

  "favorites.subtitle": "{songs} 首歌曲 · {albums} 張專輯 · {artists} 位歌手 · {playlists} 歌單",
  "favorites.playlists": "歌單",
  "favorites.filterPlaceholder": "過濾收藏…",
  "favorites.sortRecent": "最近收藏",
  "favorites.artistMeta": "{n} 張專輯",
  "favorites.emptyHint": "在任意頁面 hover 封面或曲目行，點擊愛心即可收藏到這裡",
  "favorites.emptyFilter": "沒有找到「{q}」",
  "favorites.emptySongs": "還沒有收藏歌曲",
  "favorites.emptyAlbums": "還沒有收藏專輯",
  "favorites.emptyArtists": "還沒有收藏歌手",
  "favorites.emptyPlaylists": "還沒有收藏歌單",
  "playlist.kicker": "播放清單",
  "playlist.meta": "{n} 首 · {m} 分鐘 · {updated}",
  "playlist.filterPlaceholder": "過濾歌單內歌曲…",
  "playlist.dragToReorder": "拖曳排序",
  "playlist.emptyTitle": "歌單裡沒有「{q}」",
  "playlist.emptyTryGlobal": "試試 ",
  "playlist.emptyGlobalSearch": "在整個曲庫中搜尋",

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
  "nav.search": "Search",
  "nav.favorites": "Favorites",
  "nav.settings": "Settings",

  "header.albumSubtitle": "{count} albums, sorted by recently added",
  "placeholder.title": "{name}",
  "placeholder.body": "Core scaffold is in place; this page ships in a later iteration",

  "search.placeholder": "Search songs, albums, artists, playlists…",
  "search.scopeAll": "All",
  "search.recent": "Recent searches",
  "search.emptyTitle": "No results for “{q}”",
  "search.emptyBody": "Check the spelling, or try another form of the artist or album name",
  "search.clear": "Clear",

  "settings.secLibrary": "Library",
  "settings.secPlayback": "Playback",
  "settings.secLyrics": "Lyrics",
  "settings.secAppearance": "Appearance & Language",
  "settings.secAbout": "About",
  "settings.musicFolders": "Music folders",
  "settings.addFolder": "Add folder…",
  "settings.removeFolder": "Remove folder",
  "settings.folderTracks": "{n} tracks",
  "settings.statusScanned": "Scanned",
  "settings.statusWatching": "Watching",
  "settings.watch": "Watch for file changes",
  "settings.watchDesc": "Update the library automatically when folder contents change",
  "settings.cloud": "Cloud placeholder detection",
  "settings.cloudDesc": "Skip placeholder files not yet downloaded locally, avoiding cloud fetches",
  "settings.loudness": "Loudness normalization",
  "settings.loudnessDesc": "Adjusts per-track gain only; does not compress dynamic range",
  "settings.onlineLyrics": "Online lyrics source",
  "settings.onlineLyricsDesc": "Fetch word-by-word lyrics from AMLL TTML DB; local lyrics take priority",
  "settings.wordByWord": "Word-by-word highlight",
  "settings.wordByWordDesc": "When off, highlights per line to reduce power usage",
  "settings.appearance": "Appearance",
  "settings.appearanceDesc": "In system mode, switches automatically between sunrise and sunset",
  "settings.language": "Interface language",
  "settings.languageDesc": "Applies to all interface text after a restart",
  "settings.appName": "Shannon Player",
  "settings.aboutTagline": "Free forever, no paid features",
  "settings.sourceCode": "Source code",
  "settings.backers": "Backers",
  "settings.donate": "Donate",

  "firstRun.welcomeTitle": "Welcome to Shannon Player",
  "firstRun.welcomeBody": "Your music stays local; the player just helps you hear it.\nFirst, tell me where your music lives.",
  "firstRun.addFolder": "Add music folder",
  "firstRun.formats": "Supports FLAC · MP3 · AAC · OGG · WAV and more — or just drag a folder in",
  "firstRun.scanningTitle": "Organizing your library",
  "firstRun.foundLabel": "Found",
  "firstRun.foundUnit": "songs · {m} albums",
  "firstRun.background": "Continue in background",
  "firstRun.cancel": "Cancel",
  "firstRun.doneTitle": "Your library is ready",
  "firstRun.doneBody": "{n} songs · {m} albums · {a} artists\nNew files are added automatically — no need to scan again",
  "firstRun.startListening": "Start listening",

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
  "artists.subtitle": "{n} artists",
  "artists.filterPlaceholder": "Filter artists…",
  "artists.emptyTitle": "No artists for “{q}”",
  "artists.emptyBody": "Check the spelling or try another artist name",

  "songs.subtitle": "{n} songs · {h} hr {m} min",
  "songs.sortMenu": "Sort by",
  "songs.sortByTitle": "By Title",
  "songs.sortByArtist": "By Artist",
  "songs.sortRecent": "Recently Added",
  "songs.filterPlaceholder": "Filter all songs…",
  "songs.filterClear": "Clear and close",
  "songs.colArtist": "Artist",
  "songs.groupMeta": "{albums} albums · {n} songs",
  "songs.emptyTitle": "No results for “{q}”",
  "songs.emptyBody": "Check the spelling, or try an artist or album name",

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
  "menu.removeFromPlaylist": "Remove from Playlist",

  "favorites.subtitle": "{songs} songs · {albums} albums · {artists} artists · {playlists} playlists",
  "favorites.playlists": "Playlists",
  "favorites.filterPlaceholder": "Filter favorites…",
  "favorites.sortRecent": "Recently favorited",
  "favorites.artistMeta": "{n} albums",
  "favorites.emptyHint": "Hover any cover or track row and tap the heart to save it here",
  "favorites.emptyFilter": "No results for “{q}”",
  "favorites.emptySongs": "No favorite songs yet",
  "favorites.emptyAlbums": "No favorite albums yet",
  "favorites.emptyArtists": "No favorite artists yet",
  "favorites.emptyPlaylists": "No favorite playlists yet",
  "playlist.kicker": "Playlist",
  "playlist.meta": "{n} songs · {m} min · {updated}",
  "playlist.filterPlaceholder": "Filter songs in playlist…",
  "playlist.dragToReorder": "Drag to reorder",
  "playlist.emptyTitle": "No “{q}” in this playlist",
  "playlist.emptyTryGlobal": "Try ",
  "playlist.emptyGlobalSearch": "searching the whole library",

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
  "nav.search": "検索",
  "nav.favorites": "お気に入り",
  "nav.settings": "設定",

  "header.albumSubtitle": "{count} 枚のアルバム・追加日順",
  "placeholder.title": "「{name}」ページ",
  "placeholder.body": "コア構造は完成済み。このページは今後の反復で実装します",

  "search.placeholder": "曲・アルバム・アーティスト・プレイリストを検索…",
  "search.scopeAll": "すべて",
  "search.recent": "最近の検索",
  "search.emptyTitle": "「{q}」は見つかりません",
  "search.emptyBody": "スペルを確認するか、アーティスト名やアルバム名の別の表記をお試しください",
  "search.clear": "クリア",

  "settings.secLibrary": "ライブラリ",
  "settings.secPlayback": "再生",
  "settings.secLyrics": "歌詞",
  "settings.secAppearance": "外観と言語",
  "settings.secAbout": "情報",
  "settings.musicFolders": "音楽フォルダ",
  "settings.addFolder": "フォルダを追加…",
  "settings.removeFolder": "フォルダを削除",
  "settings.folderTracks": "{n} 曲",
  "settings.statusScanned": "スキャン済み",
  "settings.statusWatching": "監視中",
  "settings.watch": "ファイル変更を監視",
  "settings.watchDesc": "フォルダの内容が変わると自動でライブラリを更新",
  "settings.cloud": "クラウドのプレースホルダー検出",
  "settings.cloudDesc": "未ダウンロードのプレースホルダーをスキップし、クラウド取得を回避",
  "settings.loudness": "ラウドネス正規化",
  "settings.loudnessDesc": "曲ごとのゲインのみ調整し、ダイナミックレンジは圧縮しません",
  "settings.onlineLyrics": "オンライン歌詞ソース",
  "settings.onlineLyricsDesc": "AMLL TTML DB から逐語歌詞を取得。ローカル歌詞を優先します",
  "settings.wordByWord": "逐語ハイライト",
  "settings.wordByWordDesc": "オフにすると行単位でハイライトし、消費電力を抑えます",
  "settings.appearance": "外観",
  "settings.appearanceDesc": "システム連動時は日の出・日の入りで自動的に切り替わります",
  "settings.language": "表示言語",
  "settings.languageDesc": "再起動後、すべての UI テキストに反映されます",
  "settings.appName": "シャノンプレーヤー",
  "settings.aboutTagline": "永久無料、有料機能なし",
  "settings.sourceCode": "ソースコード",
  "settings.backers": "サポーター一覧",
  "settings.donate": "投げ銭で支援",

  "firstRun.welcomeTitle": "シャノンプレーヤーへようこそ",
  "firstRun.welcomeBody": "音楽はローカルに保存されたまま。プレーヤーはそれを聴くお手伝いをするだけ。\nまず、音楽の場所を教えてください。",
  "firstRun.addFolder": "音楽フォルダを追加",
  "firstRun.formats": "FLAC · MP3 · AAC · OGG · WAV などに対応。フォルダをドラッグしても OK",
  "firstRun.scanningTitle": "ライブラリを整理中",
  "firstRun.foundLabel": "見つかった",
  "firstRun.foundUnit": "曲 · {m} アルバム",
  "firstRun.background": "バックグラウンドで続行",
  "firstRun.cancel": "キャンセル",
  "firstRun.doneTitle": "ライブラリの準備ができました",
  "firstRun.doneBody": "{n} 曲 · {m} アルバム · {a} アーティスト\n新しいファイルは自動で追加され、再スキャンは不要です",
  "firstRun.startListening": "聴き始める",

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
  "artists.subtitle": "{n} 組のアーティスト",
  "artists.filterPlaceholder": "アーティストを絞り込み…",
  "artists.emptyTitle": "「{q}」のアーティストは見つかりません",
  "artists.emptyBody": "スペルを確認するか、別のアーティスト名で試してください",

  "songs.subtitle": "{n} 曲 · {h} 時間 {m} 分",
  "songs.sortMenu": "並べ替え",
  "songs.sortByTitle": "タイトル順",
  "songs.sortByArtist": "アーティスト順",
  "songs.sortRecent": "最近追加した順",
  "songs.filterPlaceholder": "すべての曲をフィルタ…",
  "songs.filterClear": "クリアして閉じる",
  "songs.colArtist": "アーティスト",
  "songs.groupMeta": "{albums} 枚のアルバム · {n} 曲",
  "songs.emptyTitle": "「{q}」は見つかりません",
  "songs.emptyBody": "スペルを確認するか、アーティスト名やアルバム名で試してください",

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
  "menu.removeFromPlaylist": "プレイリストから削除",

  "favorites.subtitle": "{songs} 曲 · {albums} アルバム · {artists} アーティスト · {playlists} プレイリスト",
  "favorites.playlists": "プレイリスト",
  "favorites.filterPlaceholder": "お気に入りを絞り込み…",
  "favorites.sortRecent": "最近のお気に入り",
  "favorites.artistMeta": "{n} アルバム",
  "favorites.emptyHint": "任意のページでカバーや曲の行にホバーし、ハートをタップすればここに保存されます",
  "favorites.emptyFilter": "「{q}」は見つかりません",
  "favorites.emptySongs": "お気に入りの曲はまだありません",
  "favorites.emptyAlbums": "お気に入りのアルバムはまだありません",
  "favorites.emptyArtists": "お気に入りのアーティストはまだありません",
  "favorites.emptyPlaylists": "お気に入りのプレイリストはまだありません",
  "playlist.kicker": "プレイリスト",
  "playlist.meta": "{n} 曲 · {m} 分 · {updated}",
  "playlist.filterPlaceholder": "プレイリスト内を絞り込み…",
  "playlist.dragToReorder": "ドラッグして並べ替え",
  "playlist.emptyTitle": "プレイリストに「{q}」はありません",
  "playlist.emptyTryGlobal": "",
  "playlist.emptyGlobalSearch": "ライブラリ全体を検索",

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
