// The 66-book canon in order: slug (as WACS repo names spell it), USFM id,
// English name, chapter count. Coverage maps and tS stitching both key on it.
const ROWS = `
gen GEN Genesis 50|exo EXO Exodus 40|lev LEV Leviticus 27|num NUM Numbers 36|deu DEU Deuteronomy 34|
jos JOS Joshua 24|jdg JDG Judges 21|rut RUT Ruth 4|1sa 1SA 1_Samuel 31|2sa 2SA 2_Samuel 24|
1ki 1KI 1_Kings 22|2ki 2KI 2_Kings 25|1ch 1CH 1_Chronicles 29|2ch 2CH 2_Chronicles 36|ezr EZR Ezra 10|
neh NEH Nehemiah 13|est EST Esther 10|job JOB Job 42|psa PSA Psalms 150|pro PRO Proverbs 31|
ecc ECC Ecclesiastes 12|sng SNG Song_of_Songs 8|isa ISA Isaiah 66|jer JER Jeremiah 52|lam LAM Lamentations 5|
ezk EZK Ezekiel 48|dan DAN Daniel 12|hos HOS Hosea 14|jol JOL Joel 3|amo AMO Amos 9|
oba OBA Obadiah 1|jon JON Jonah 4|mic MIC Micah 7|nam NAM Nahum 3|hab HAB Habakkuk 3|
zep ZEP Zephaniah 3|hag HAG Haggai 2|zec ZEC Zechariah 14|mal MAL Malachi 4|
mat MAT Matthew 28|mrk MRK Mark 16|luk LUK Luke 24|jhn JHN John 21|act ACT Acts 28|
rom ROM Romans 16|1co 1CO 1_Corinthians 16|2co 2CO 2_Corinthians 13|gal GAL Galatians 6|eph EPH Ephesians 6|
php PHP Philippians 4|col COL Colossians 4|1th 1TH 1_Thessalonians 5|2th 2TH 2_Thessalonians 3|1ti 1TI 1_Timothy 6|
2ti 2TI 2_Timothy 4|tit TIT Titus 3|phm PHM Philemon 1|heb HEB Hebrews 13|jas JAS James 5|
1pe 1PE 1_Peter 5|2pe 2PE 2_Peter 3|1jn 1JN 1_John 5|2jn 2JN 2_John 1|3jn 3JN 3_John 1|
jud JUD Jude 1|rev REV Revelation 22`;

export const BOOKS = ROWS.split("|").map((row, i) => {
  const [slug, usfm, name, chapters] = row.trim().split(/\s+/);
  return { slug, usfm, name: name.replaceAll("_", " "), chapters: Number(chapters), nt: i >= 39, index: i };
});

const BY_SLUG = new Map(BOOKS.map((b) => [b.slug, b]));
// A few spellings seen in the wild besides the canonical slug.
for (const [alias, slug] of [["mar", "mrk"], ["joh", "jhn"], ["jas", "jas"], ["sol", "sng"], ["eze", "ezk"], ["jdgs", "jdg"]]) {
  BY_SLUG.set(alias, BY_SLUG.get(slug));
}

export const bookBySlug = (slug) => BY_SLUG.get(String(slug).toLowerCase()) ?? null;

const BY_USFM = new Map(BOOKS.map((b) => [b.usfm, b]));
export const bookByUsfm = (id) => BY_USFM.get(String(id).toUpperCase()) ?? null;

/**
 * The book a WACS repo name is about, or null for a whole-Bible or unknown
 * repo. Names look like `{lang}_{book}_{track}_{rtype}`; the language code can
 * itself contain `_`-free dashes, so take the first token that is a book.
 */
export function bookFromRepoName(name) {
  for (const token of name.toLowerCase().split("_").slice(1)) {
    const book = bookBySlug(token);
    if (book) return book;
  }
  return null;
}
