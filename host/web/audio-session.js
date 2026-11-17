// This page entry runs before runtime activation, including on interaction pages (LLP 1096 D7).
{
  const session = document.getElementById('exact-root')?.dataset.audioSession;
  // The Audio Session draft has no mixing playback type.
  if (session && navigator.audioSession) navigator.audioSession.type = session === 'playback-mix' ? 'ambient' : session;
}
