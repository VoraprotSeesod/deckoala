/** Canonical Marp block/directive strings, shared by the toolbar inserter
 * (BRIEF-0012) so the toolbar, guide and themes never drift apart. The class
 * directives are the SAME ones the deckoala themes style (`.columns`/`.center`
 * from BRIEF-0009c) and the guide documents (BRIEF-0009d). */

export const CLASS_COLUMNS = '<!-- _class: columns -->';
export const CLASS_CENTER = '<!-- _class: center -->';

export const BLOCK = {
	slideBreak: '---',
	columns: `${CLASS_COLUMNS}\n\n## Left\n\nText\n\n## Right\n\nText`,
	center: `${CLASS_CENTER}\n\n# Centered`,
	table: '| Column A | Column B |\n| --- | --- |\n| 1 | 2 |',
	code: '```\ncode\n```',
	math: '$$\nE = mc^2\n$$',
	note: '<!-- Speaker note: only you see this in presenter view. -->'
} as const;

export type BlockKind = keyof typeof BLOCK;
