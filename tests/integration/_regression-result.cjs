'use strict';

const PREFIX = 'VERENU_TEST_RESULT=';

function finish({
  status,
  expected,
  observed,
  regressionArea,
  measurements = {},
  baseline = {},
  failureKind = null,
  regressionStatus = 'unknown',
  skipReason = null,
}) {
  const payload = {
    status,
    expected,
    observed,
    regression_area: regressionArea,
    measurements,
    baseline,
    failure_kind: failureKind,
    regression_status: regressionStatus,
    skip_reason: skipReason,
  };
  console.log(PREFIX + JSON.stringify(payload));
  if (status === 'failed') process.exitCode = 1;
}

function message(error) {
  return error instanceof Error ? error.message : String(error);
}

module.exports = { finish, message };

