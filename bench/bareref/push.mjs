export async function exactPush(session, committed) {
  const { id } = await session.find('change-station');
  // session.tap also lands answers with Agent.clock, which would enter the measured interval.
  await session.op({ op: 'tap', id });
  await committed();
  await session.clock('settle');
}
