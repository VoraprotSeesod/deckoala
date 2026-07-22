// Bold Deckoala theme — light, oversized type and heavy headings for
// high-impact keynote slides (BRIEF-0009c). Same brand palette and Thai-capable
// fonts as the default; larger scale and stronger weights.
export const themeDeckoalaBold = `/* @theme deckoala-bold */
@import 'default';

section {
	background: #f8f8ff;
	color: #0b1215;
	font-family: 'Inter', 'Noto Sans Thai', system-ui, sans-serif;
	font-size: 34px;
	line-height: 1.4;
	font-weight: 500;
	padding: 72px;
}

h1,
h2,
h3,
h4,
h5,
h6 {
	color: #0b1215;
	line-height: 1.1;
	font-weight: 800;
	letter-spacing: -0.01em;
}

h1 {
	font-size: 2.6em;
}

h2 {
	font-size: 1.8em;
}

a {
	color: #0b1215;
	text-decoration: underline;
	text-decoration-thickness: 0.12em;
}

strong {
	font-weight: 800;
}

code {
	background: rgba(11, 18, 21, 0.08);
	color: #0b1215;
	padding: 0.1em 0.35em;
	border-radius: 4px;
}

pre {
	background: rgba(11, 18, 21, 0.06);
	padding: 0.7em 1em;
	border-radius: 10px;
}

pre code {
	background: transparent;
	padding: 0;
}

blockquote {
	border-left: 6px solid rgba(11, 18, 21, 0.3);
	padding-left: 0.8em;
	color: rgba(11, 18, 21, 0.8);
	font-weight: 600;
}

table th {
	background: rgba(11, 18, 21, 0.08);
}

section::after {
	color: rgba(11, 18, 21, 0.45);
}

section.columns {
	column-count: 2;
	column-gap: 1.5em;
}

section.center {
	text-align: center;
}

section.center img {
	margin-left: auto;
	margin-right: auto;
}
`;
