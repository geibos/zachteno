/**
 * Фиксированный список шрифтов для проверки наличия (design D14).
 * Порядок важен: результат — битовая маска по индексам этого списка.
 * Список только дополнять в конец, иначе маски разных версий не сравнить.
 */
export const FONT_LIST: readonly string[] = [
	// Windows
	'Arial', 'Arial Black', 'Arial Narrow', 'Arial Rounded MT Bold', 'Bahnschrift', 'Calibri', 'Calibri Light',
	'Cambria', 'Cambria Math', 'Candara', 'Comic Sans MS', 'Consolas', 'Constantia', 'Corbel', 'Courier New',
	'Ebrima', 'Franklin Gothic Medium', 'Gabriola', 'Gadugi', 'Georgia', 'HoloLens MDL2 Assets', 'Impact',
	'Ink Free', 'Javanese Text', 'Leelawadee UI', 'Lucida Console', 'Lucida Sans Unicode', 'Malgun Gothic',
	'Marlett', 'Microsoft Himalaya', 'Microsoft JhengHei', 'Microsoft New Tai Lue', 'Microsoft PhagsPa',
	'Microsoft Sans Serif', 'Microsoft Tai Le', 'Microsoft YaHei', 'Microsoft Yi Baiti', 'MingLiU-ExtB',
	'Mongolian Baiti', 'MS Gothic', 'MV Boli', 'Myanmar Text', 'Nirmala UI', 'Palatino Linotype',
	'Segoe MDL2 Assets', 'Segoe Print', 'Segoe Script', 'Segoe UI', 'Segoe UI Emoji', 'Segoe UI Historic',
	'Segoe UI Symbol', 'SimSun', 'Sitka Text', 'Sylfaen', 'Symbol', 'Tahoma', 'Times New Roman',
	'Trebuchet MS', 'Verdana', 'Webdings', 'Wingdings', 'Yu Gothic', 'Agency FB', 'Algerian', 'Baskerville Old Face',
	'Bauhaus 93', 'Bell MT', 'Berlin Sans FB', 'Bernard MT Condensed', 'Blackadder ITC', 'Bodoni MT',
	'Book Antiqua', 'Bookman Old Style', 'Bradley Hand ITC', 'Britannic Bold', 'Broadway', 'Brush Script MT',
	'Californian FB', 'Calisto MT', 'Castellar', 'Centaur', 'Century', 'Century Gothic', 'Century Schoolbook',
	'Chiller', 'Colonna MT', 'Cooper Black', 'Copperplate Gothic Bold', 'Curlz MT', 'Edwardian Script ITC',
	'Elephant', 'Engravers MT', 'Eras Bold ITC', 'Felix Titling', 'Footlight MT Light', 'Forte', 'Freestyle Script',
	'French Script MT', 'Garamond', 'Gigi', 'Gill Sans MT', 'Gloucester MT Extra Condensed', 'Goudy Old Style',
	'Haettenschweiler', 'Harlow Solid Italic', 'Harrington', 'High Tower Text', 'Imprint MT Shadow',
	'Informal Roman', 'Jokerman', 'Juice ITC', 'Kristen ITC', 'Kunstler Script', 'Lucida Bright',
	'Lucida Calligraphy', 'Lucida Fax', 'Lucida Handwriting', 'Magneto', 'Maiandra GD', 'Matura MT Script Capitals',
	'Mistral', 'Modern No. 20', 'Monotype Corsiva', 'MS Outlook', 'MS Reference Sans Serif', 'MS Reference Specialty',
	'Niagara Engraved', 'Old English Text MT', 'Onyx', 'Palace Script MT', 'Papyrus', 'Parchment', 'Perpetua',
	'Playbill', 'Poor Richard', 'Pristina', 'Rage Italic', 'Ravie', 'Rockwell', 'Script MT Bold', 'Showcard Gothic',
	'Snap ITC', 'Stencil', 'Tempus Sans ITC', 'Tw Cen MT', 'Viner Hand ITC', 'Vivaldi', 'Vladimir Script',
	'Wide Latin', 'Aptos', 'Cascadia Code', 'Cascadia Mono', 'Sakkal Majalla', 'Simplified Arabic', 'Traditional Arabic',
	'Arabic Typesetting', 'Leelawadee', 'DengXian', 'FangSong', 'KaiTi', 'SimHei', 'NSimSun', 'Meiryo', 'MS Mincho',
	'MS PGothic', 'Batang', 'Gulim', 'Dotum', 'Gungsuh', 'Microsoft Uighur', 'Iskoola Pota', 'Kalinga', 'Shruti',
	'Tunga', 'Vrinda', 'Raavi', 'Latha', 'Gautami', 'Kartika', 'Mangal', 'Aparajita', 'Kokila', 'Utsaah',
	// macOS
	'American Typewriter', 'Andale Mono', 'Apple Chancery', 'Apple Color Emoji', 'Apple SD Gothic Neo',
	'AppleGothic', 'AppleMyungjo', 'Avenir', 'Avenir Next', 'Avenir Next Condensed', 'Baskerville', 'Big Caslon',
	'Bodoni 72', 'Bradley Hand', 'Chalkboard', 'Chalkboard SE', 'Chalkduster', 'Charter', 'Cochin', 'Copperplate',
	'Courier', 'Didot', 'DIN Alternate', 'DIN Condensed', 'Futura', 'Geneva', 'Gill Sans', 'Helvetica',
	'Helvetica Neue', 'Herculanum', 'Hiragino Kaku Gothic ProN', 'Hiragino Mincho ProN', 'Hiragino Sans',
	'Hoefler Text', 'Iowan Old Style', 'Kailasa', 'Kefa', 'Kohinoor Devanagari', 'Krungthep', 'Lucida Grande',
	'Luminari', 'Marker Felt', 'Menlo', 'Monaco', 'Noteworthy', 'Optima', 'Palatino', 'Papyrus Condensed',
	'Phosphate', 'PingFang SC', 'PingFang TC', 'Rockwell Nova', 'Savoye LET', 'SignPainter', 'Skia',
	'Snell Roundhand', 'SF Pro', 'SF Mono', 'New York', 'STIXGeneral', 'Thonburi', 'Trattatello', 'Zapfino',
	'Arial Hebrew', 'Al Nile', 'Ayuthaya', 'Damascus', 'Farah', 'Galvji', 'Mishafi', 'Muna', 'Nadeem', 'Sana',
	'Songti SC', 'STHeiti', 'Toppan Bunkyu Gothic', 'Tsukushi A Round Gothic', 'YuGothic', 'YuMincho',
	// Linux и свободные
	'DejaVu Sans', 'DejaVu Sans Mono', 'DejaVu Serif', 'Liberation Sans', 'Liberation Serif', 'Liberation Mono',
	'Ubuntu', 'Ubuntu Mono', 'Cantarell', 'Noto Sans', 'Noto Serif', 'Noto Sans Mono', 'Noto Color Emoji',
	'Droid Sans', 'Droid Sans Mono', 'Droid Serif', 'FreeSans', 'FreeSerif', 'FreeMono', 'Nimbus Sans',
	'Nimbus Roman', 'Nimbus Mono PS', 'URW Gothic', 'URW Bookman', 'P052', 'C059', 'Z003', 'D050000L',
	'Bitstream Vera Sans', 'Bitstream Vera Serif', 'Bitstream Vera Sans Mono', 'Linux Libertine', 'Linux Biolinum',
	'Open Sans', 'Roboto', 'Roboto Mono', 'Roboto Condensed', 'Source Sans Pro', 'Source Code Pro', 'Fira Sans',
	'Fira Mono', 'Fira Code', 'Hack', 'Inconsolata', 'JetBrains Mono', 'Lato', 'Montserrat', 'Oxygen', 'PT Sans',
	'PT Serif', 'PT Mono', 'Carlito', 'Caladea', 'Gentium', 'Gentium Plus', 'Comfortaa', 'Terminus',
	'WenQuanYi Micro Hei', 'WenQuanYi Zen Hei', 'Lohit Devanagari', 'Padauk', 'Khmer OS', 'Waree', 'Kinnari',
	// Android / Chrome OS
	'Google Sans', 'Product Sans', 'Samsung Sans', 'SamsungOne', 'Roboto Flex', 'Noto Sans CJK SC',
	// офисные пакеты и прочее распространённое
	'Arimo', 'Tinos', 'Cousine', 'Inter', 'IBM Plex Sans', 'IBM Plex Mono', 'IBM Plex Serif', 'Merriweather',
	'Playfair Display', 'Raleway', 'Nunito', 'Poppins', 'Exo 2', 'Yandex Sans', 'YS Text', 'YS Display'
];
