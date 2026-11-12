import type { Answer, Sources, StreamEvent } from './app.contract.d.ts';

// Use your HTTPS API here; loopback HTTP is only for the bundled test server.
const API = import.meta.env.SOCKET_API ?? 'https://api.example.com';
const SOCKET = import.meta.env.SOCKET_URL ?? 'wss://api.example.com';
export const grants = `net.fetch ${API}\nnet.websocket ${SOCKET}`;
const tickets = new Map<number, string>();
let currentAttempt = 0;

// With OAuth: const authorizedFetch = (store, url, options) => provider.fetch(store, url, options).
const authorizedFetch = (_store: Parameters<Sources['ticket']>[1], url: string, options: RequestInit) => fetch(url, {
  ...options, headers: { authorization: 'Bearer fixture-only' }, redirect: 'error',
});

const sources: Sources = {
  ticket: async ([attempt], store) => {
    currentAttempt = attempt;
    tickets.clear();
    const response = await authorizedFetch(store, API + '/events/ticket', { method: 'POST' });
    if (!response.ok) throw new Error(`ticket request failed (${response.status})`);
    const { ticket } = await response.json();
    if (typeof ticket !== 'string' || !ticket) throw new Error('the ticket endpoint returned no ticket');
    if (attempt === currentAttempt) tickets.set(attempt, ticket);
    return attempt;
  },
  events: ([revision]) => {
    const ticket = tickets.get(revision);
    tickets.delete(revision);
    if (!ticket) throw new Error('connect again to request a fresh ticket');
    // This source opens synchronously: no await before its stream fetch (LLP 1016.000).
    return fetch(SOCKET + '/events?ticket=' + encodeURIComponent(ticket), {
      exactProtocols: ['events.v1'],
      exactStream(event: StreamEvent) {
        return {
          status: event.type === 'message' ? 'connected' : event.type,
          protocol: event.protocol ?? '',
          text: event.type === 'message' ? String(event.data) : '',
        };
      },
    });
  },
};
export const answer: Answer = (name, args, store, storage, native) => sources[name](args, store, storage, native);
