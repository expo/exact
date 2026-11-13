import type { Answer } from './app.contract.d.ts';

// The harness prepends the carrier's appId and grants declarations.
const directory = 'app:/data/backups';
const path = directory + '/background.txt';
const encode = (text: string) => new Uint8Array(Array.from(text, c => c.charCodeAt(0)));
const decode = (bytes: ArrayBuffer) => Array.from(new Uint8Array(bytes), c => String.fromCharCode(c)).join('');
let failure = '';
const code = (error: unknown) => String((error as { code?: string }).code ?? 'unknown');

export const answer: Answer = (_source, [command], _store, storage) => {
  if (command === 'init') {
    void storage.fs.mkdir(directory).catch(error => { failure = code(error); });
  } else if (command === 'read') {
    return storage.fs.readFile(path).then(decode, code);
  } else if (command === 'code') {
    return failure;
  } else if (command === 'chain') {
    void storage.fs.atomicWriteFile(path, encode('start'))
      .then(() => storage.fs.atomicWriteFile(path, encode('end')))
      .then(() => storage.fs.readFile(path))
      .catch(error => { failure = code(error); });
  } else if (command === 'fail') {
    void storage.fs.readFile(directory + '/missing.txt').catch(error => { failure = code(error); });
  } else {
    void storage.fs.atomicWriteFile(path, encode(command)).catch(error => { failure = code(error); });
  }
  return command;
};
