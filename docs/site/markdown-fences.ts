const FENCE = /^ {0,3}(`{3,}|~{3,})(.*)$/;

export function proseLines(lines: string[]) {
  const indices: number[] = [];
  let open = '';
  let openingLine = 0;
  lines.forEach((line, i) => {
    const fence = FENCE.exec(line);
    if (open) {
      if (fence && fence[1].startsWith(open) && !fence[2].trim()) open = '';
    } else if (fence && !(fence[1][0] === '`' && fence[2].includes('`'))) {
      open = fence[1];
      openingLine = i + 1;
    } else {
      indices.push(i);
    }
  });
  return { indices, unclosed: open ? openingLine : null };
}
