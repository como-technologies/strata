/**
 * @name Cleartext logging of sensitive information
 * @description Logging sensitive information in plaintext can
 *              expose it to an attacker.
 * @kind path-problem
 * @problem.severity warning
 * @security-severity 7.5
 * @precision high
 * @id rust/cleartext-logging
 * @tags security
 *       external/cwe/cwe-312
 *       external/cwe/cwe-359
 *       external/cwe/cwe-532
 *       strata
 */

// The query rust/cleartext-logging of CodeQL, with two changes. See "CodeQL" in
// docs/src/development/build-test.md.
//
// First, a name that the heuristic of CodeQL reads only as account information is no source. Such
// a name holds `uid`, `account`, a user name, or a session id. Strata prints the uid and the name
// of each object of Palette by design. A secret, a password, an API key, a certificate, and a
// private value stay sources.
//
// Second, this copy does not observe the diff of a pull request. The query of CodeQL does, and in
// a pull request it then gives only the alerts in the lines of the diff. The crate of the test of
// the rule is in no diff.
//
// The tag `strata` lets the config of .github/codeql/rust.yml keep this query, and drop the query
// of CodeQL with the same id.
import rust
import codeql.rust.dataflow.DataFlow
import codeql.rust.dataflow.TaintTracking
import codeql.rust.security.CleartextLoggingExtensions
import codeql.rust.security.SensitiveData

/**
 * Holds when the heuristic of CodeQL reads `data` only as account information, such as a uid or a
 * name of Palette.
 */
predicate isOnlyAccountInfo(SensitiveData data) {
  forex(SensitiveDataClassification c | c = data.getClassification() |
    c = SensitiveDataClassification::id()
  )
}

/**
 * The flow of the rule: from a source of sensitive data to a sink that writes a log.
 */
module CleartextLoggingConfig implements DataFlow::ConfigSig {
  import CleartextLogging

  // the first change to the query of CodeQL
  predicate isSource(DataFlow::Node source) {
    source instanceof Source and not isOnlyAccountInfo(source)
  }

  predicate isSink(DataFlow::Node sink) { sink instanceof Sink }

  predicate isBarrier(DataFlow::Node barrier) { barrier instanceof Barrier }

  predicate isBarrierIn(DataFlow::Node node) {
    // a source is a barrier, so a path starts at the source that is nearest to the sink
    isSource(node)
  }

  predicate isAdditionalFlowStep(DataFlow::Node node1, DataFlow::Node node2) {
    // the value of `a` flows to `&a`
    node2.asExpr().(RefExpr).getExpr() = node1.asExpr()
  }

  predicate allowImplicitRead(DataFlow::Node node, DataFlow::ContentSet c) {
    // a sink reads the content of a tuple
    isSink(node) and
    c.getAReadContent() instanceof DataFlow::TuplePositionContent
  }
}

module CleartextLoggingFlow = TaintTracking::Global<CleartextLoggingConfig>;

import CleartextLoggingFlow::PathGraph

from CleartextLoggingFlow::PathNode source, CleartextLoggingFlow::PathNode sink
where CleartextLoggingFlow::flowPath(source, sink)
select sink.getNode(), source, sink, "This operation writes $@ to a log file.", source,
  source.getNode().toString()
