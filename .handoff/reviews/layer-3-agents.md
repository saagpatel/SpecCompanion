CHANGES

Reviewed only commit `dd8b97d44d1dab9a6c44495e802fe746b773eda6` against its parent, using the architecture contract and its revisions. Each input below uses `- [R-001] Export timestamp is 30.` in `spec.md`. Ignored probes confirmed that each indexed test receives `AuditVerdict::Accepted`; supplying its passing outcome to `score_line_at_revision` produces `Badge::Proven`. Rust examples were also compiled and passed as real libtest tests. Python examples were invoked directly, and the JavaScript callback was invoked with a minimal `test` shim; those are execution probes, not full pytest/Jest integrations.

## Major: JavaScript regex literals become fabricated assertion evidence

**Location:** `living-spec/src/agents/syntax.rs:122-135`, consumed by `living-spec/src/agents/auditor.rs:206-253`.

**Failing input — `test.js`:**

```javascript
test('export timestamp', () => {
    /expect(export_timestamp(30)).toBe(30)/.test('x');
});
```

The lexer treats the regex delimiters as ordinary symbols and its contents as executable tokens. The auditor discovers the apparent `expect` chain inside the regex and accepts it. The callback actually executes no assertion and never calls `export_timestamp`; it returns successfully even with both `expect` and `export_timestamp` undefined. A passing result can therefore turn an assertion-free test into a green requirement.

**Suggested fix:** Recognize JavaScript regex literals and exclude their contents from executable tokens. If the lexer cannot distinguish a regex from division conservatively, challenge that test as unsupported instead of scanning the ambiguous contents for assertions.

## Major: Irrefutable Rust binding patterns pass the placeholder gate

**Location:** `living-spec/src/agents/auditor.rs:381-397`, especially `389-394`.

**Failing input — `tests/export.rs`:**

```rust
fn export_timestamp(_: u32) -> u32 { 0 }

#[test]
fn test_export_timestamp() {
    assert!(matches!(export_timestamp(30), any_value));
}
```

`any_value` is a binding pattern: it matches every value, so this test passes with the incorrect output `0`. The analyzer rejects a lone `_` but strips this equally irrefutable pattern, leaving the relevant function call as apparently meaningful assertion evidence. This is a placeholder assertion accepted as green evidence.

**Suggested fix:** Preserve the pattern when checking whether `matches!` constrains its scrutinee. Challenge binding-only and other irrefutable patterns; challenge patterns whose refutability cannot be established by the supported syntax.

## Major: A placeholder assertion can supply the expected panic

**Location:** `living-spec/src/agents/auditor.rs:122-137`.

**Failing input — `tests/export.rs`:**

```rust
fn export_timestamp(_: u32) -> u32 { 0 }

#[test]
#[should_panic]
fn test_export_timestamp() {
    export_timestamp(30);
    assert!(false);
}
```

Libtest reports this test as passed because the literal assertion panics. The function result is ignored. Before analyzing assertions, the `should_panic` branch adds every ordinary call as a separate assertion expression. That synthesized expression survives the placeholder filter even though the only actual assertion is `assert!(false)`. Thus the existing rejection of unconditional `panic!` can be bypassed with an unconditional assertion failure.

**Suggested fix:** Do not promote arbitrary calls in a `should_panic` body to assertion evidence when another operation can supply the panic. Accept only the supported direct expected-panic call shape, or conservatively challenge bodies with additional assertions or other possible panic sources.

## Major: Multiline decorators bypass the decorated-test safeguard

**Location:** `living-spec/src/agents/test_index.rs:344-353`; the omitted decorator bypasses `living-spec/src/agents/auditor.rs:27-29`.

**Failing input — `test_export.py`:**

```python
def skip_assertion():
    return lambda fn: lambda: None

def export_timestamp(value):
    raise RuntimeError('the decorated test never calls this')

@skip_assertion(
)
def test_export_timestamp():
    assert export_timestamp(30) == 30
```

The backward scan includes only immediately preceding lines starting with `@`. Here the previous line is `)`, so the indexed source begins at `def`, and the auditor accepts the original body as undecorated. Calling the collected function actually invokes the replacement lambda, which returns successfully without executing the body. The decorator is also absent from the fingerprint, so edits to it do not invalidate the source identity.

**Suggested fix:** Include complete decorator expressions in the indexed source span and fingerprint, including multiline expressions. Mark decorated tests explicitly and challenge them when decorator semantics are unsupported; do not infer decoration solely from the first character of a possibly truncated source span.

## Major: Unsupported Python context managers can swallow failed assertions

**Location:** `living-spec/src/agents/auditor.rs:94-118` and `196-205`.

**Failing input — `test_export.py`:**

```python
import contextlib

def export_timestamp(value):
    return 0

def test_export_timestamp():
    with contextlib.suppress(AssertionError):
        assert export_timestamp(30) == 30
```

The comparison fails, but the context manager suppresses `AssertionError`, so the test returns successfully. The unsupported-control-flow check omits `with`, and the assertion scanner accepts the nested assertion without accounting for the enclosing context. This permits a failing check to contribute green evidence. The same behavior reproduces with a custom context manager that suppresses `AssertionError`.

**Suggested fix:** Challenge unsupported `with` suites. Handle the explicitly supported `pytest.raises` form separately, validating its suite and expected-exception semantics instead of treating arbitrary nested assertions as unconditional evidence.
