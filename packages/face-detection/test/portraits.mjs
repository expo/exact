// The real detector in the shared vision fixture, fed by the file picker.
import { resolve } from 'node:path';
import { images, shown } from '../../barcode-detection/test/fixture.mjs';

const cases = [
  ['frontal', 1], ['turned', 1], ['tilted', 1], ['raised', 1], ['closed', 1],
  ['two', 2], ['none', 0], ['exif6', 1],
];

export function portraitSteps() {
  return cases.flatMap(([name]) => [
    'tap choose', `pick @image ${name === 'none' ? images.none : resolve(import.meta.dir, '../fixture', name + '.jpg')}`,
    'clock data', 'tap detect-faces', 'clock data', 'tree face-answer',
  ]);
}

export function checkPortraits(host, out, check, requirePose = host === 'android') {
  const answers = out.split(/(?=\[faces\] ")/).slice(1);
  let frontalBox, frontalEyes;
  cases.forEach(([name, count], i) => {
    const answer = answers[i] ?? '';
    const size = name === 'two' ? '720x480' : name === 'none' ? '400x300' : '400x480';
    check(host, name + ' dimensions and count', shown(answer, 'faces')[0] === size + ' ' + count, shown(answer, 'faces'));
    const boxes = shown(answer, 'face-box').map(line => line.split(' ').map(Number));
    const poses = shown(answer, 'face-pose'), eyes = shown(answer, 'face-eyes'), points = shown(answer, 'face-points');
    for (const [j, box] of boxes.entries()) {
      const [x, y, w, h] = box;
      const eyeLine = name === 'raised' ? 180 : 215;
      const centre = j ? 595 : name === 'turned' ? 250 : 205;
      check(host, name + ' box around the portrait', x > centre - 180 && x < centre && x + w > centre &&
        y > 40 && y < eyeLine && y + h > eyeLine && w > 100 && h > 100, box);
      if (j) check(host, 'larger portrait first', w * h < boxes[j - 1][2] * boxes[j - 1][3], box);
      const regions = (points[j] ?? '').split(' ').filter(Boolean).map(s => {
        const [type, ...coords] = s.split(',');
        return { type, coords: coords.map(Number) };
      });
      check(host, name + ' landmark locations inside the face box', regions.length > 0 && regions.every(({ coords }) =>
        coords.length >= 2 && coords.length % 2 === 0 && coords.every((v, i) =>
          i % 2 === 0 ? v >= x && v <= x + w : v >= y && v <= y + h)), regions);
      const marks = regions.filter(r => r.type === 'eye').map(r => r.coords.slice(0, 2)).sort((a, b) => a[0] - b[0]);
      if (name === 'frontal' || name === 'exif6') {
        check(host, name + ' two eye landmarks', marks.length === 2, marks);
        if (host !== 'ios-simulator') check(host, name + ' eye landmarks in upright pixels', marks.length === 2 &&
          marks[0][0] > 120 && marks[0][0] < 185 && marks[1][0] > 230 && marks[1][0] < 290 &&
          marks.every(([, py]) => py > 165 && py < 220), marks);
      }
      if (name === 'frontal') { frontalBox = box; frontalEyes = marks.flat(); }
      if (name === 'exif6') {
        check(host, 'EXIF preserves the frontal box', box.every((v, k) => Math.abs(v - frontalBox?.[k]) < 12), box);
        if (host !== 'ios-simulator') check(host, 'EXIF preserves eye locations', marks.flat().every((v, k) => Math.abs(v - frontalEyes?.[k]) < 12), marks);
      }
      const pose = (poses[j] ?? '').split(' ').map(Number);
      if (requirePose || poses[j] !== 'absent') {
        check(host, name + ' pose', pose.length === 3 && pose.every(Number.isFinite), pose);
        if (name === 'turned') check(host, 'turn to the person’s left', pose[0] > 15 && pose[0] < 55, pose);
        if (name === 'raised') check(host, 'lifted chin', pose[1] > 10 && pose[1] < 45, pose);
        if (name === 'tilted') check(host, 'right ear toward right shoulder', pose[2] > 10 && pose[2] < 45, pose);
      }
      if (host === 'android' && ['closed', 'frontal'].includes(name)) {
        const p = (eyes[j] ?? '').split(' ').map(Number);
        check(host, name + ' eye probabilities', p.length === 2 && p.every(v => name === 'closed' ? v >= 0 && v < 0.3 : v >= 0.3 && v <= 1), p);
      } else if (host !== 'android') {
        check(host, 'Android classification absent', eyes[j] === 'absent', eyes[j]);
      }
    }
    check(host, name + ' box count', boxes.length === count, boxes.length);
  });
}
