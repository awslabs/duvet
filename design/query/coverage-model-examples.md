# Duvet Coverage Model: Worked Examples

These examples illustrate the coverage model algorithms from the
[formal specification](coverage-model-spec.md). They are informational
and not tracked as requirements.

## 6. Worked Examples {#worked-examples}

### 6.1 Annotation before method signature (Java) {#example-method-signature}

```java
1:  //= spec.md#section-1              Some({Annotation})
2:  //# MUST do X                       Some({Annotation})
3:  public void foo() {                 Some({Declaration, ScopeOpen})
4:      int temp;                       Some({Declaration})
5:      doX();                          Some({Statement})  ← coverage: Hit
6:  }                                   Some({ScopeClose})
```

Phase 1: Annotation (lines 1-2) → target = line 3, properties: Some({Declaration, ScopeOpen}).

Phase 2: Line 5 is Hit. Walk backward in scope (lines 3-6):
- Line 4: Some({Declaration}) → propagate. result += {4}
- Line 3: Some({ScopeOpen}) → propagate, stop. result += {3}

Execution set = {3, 4, 5}.

Phase 3: Target (line 3) ∈ execution set → **Executed**.

### 6.2 Annotation on interface (Java) {#example-interface}

```java
1:  //= spec.md#keyring                Some({Annotation})
2:  //# MUST define OnEncrypt           Some({Annotation})
3:  public interface IKeyring {         Some({Declaration, ScopeOpen})
4:      OnEncryptOutput OnEncrypt(      Some({Declaration})
5:          OnEncryptInput input        Some({Declaration})
6:      );                              Some({Declaration})
7:  }                                   Some({ScopeClose})
```

Phase 1: Annotation (lines 1-2) → target = line 3, properties: Some({Declaration, ScopeOpen}).

Phase 2: No line in scope (3-7) has `coverage[line] == Hit`. No propagation.
Execution set = {} (for this scope).

Phase 3: Target (line 3) ∉ execution set. Target is {Declaration, ScopeOpen}
(no Statement). Scope contains no Statements → **Structural**.

### 6.3 Cross-method leakage prevention {#example-cross-method}

```java
1:  public void foo() {                 Some({Declaration, ScopeOpen})
2:      //= spec.md#section-1          Some({Annotation})
3:      //# MUST do X                   Some({Annotation})
4:      doX();                          Some({Statement})  ← coverage: Hit
5:  }                                   Some({ScopeClose})
6:                                      Some({Whitespace})
7:  public void bar() {                 Some({Declaration, ScopeOpen})
8:      doY();                          Some({Statement})  ← coverage: Hit
9:  }                                   Some({ScopeClose})
```

Phase 1: Annotation (lines 2-3) → target = line 4, properties: Some({Statement}).

Phase 2: Line 4 is Hit in scope (1-5). Walk backward:
- Line 3: Some({Annotation}) → propagate. result += {3}
- Line 2: Some({Annotation}) → propagate. result += {2}
- Line 1: Some({ScopeOpen}) → propagate, stop. result += {1}

Line 8 is Hit in scope (7-9). Walk backward:
- Line 7: Some({ScopeOpen}) → propagate, stop. result += {7}

Execution set = {1, 2, 3, 4, 7, 8}.

Phase 3: Target (line 4) ∈ execution set → **Executed**.

Note: Line 8's execution does NOT propagate to lines 5 or 6. The `ScopeClose`
at line 5 and the scope boundary at line 7 prevent leakage.

### 6.4 Variable declaration without initializer {#example-var-decl}

```java
1:  public void foo() {                 Some({Declaration, ScopeOpen})
2:      //= spec.md#section-1          Some({Annotation})
3:      //# MUST compute X              Some({Annotation})
4:      int result;                     Some({Declaration})
5:      result = computeX();            Some({Statement})  ← coverage: Hit
6:  }                                   Some({ScopeClose})
```

Phase 1: Annotation (lines 2-3) → target = line 4, properties: Some({Declaration}).

Phase 2: Line 5 is Hit. Walk backward in scope (1-6):
- Line 4: Some({Declaration}) → propagate. result += {4}
- Line 3: Some({Annotation}) → propagate. result += {3}
- Line 2: Some({Annotation}) → propagate. result += {2}
- Line 1: Some({ScopeOpen}) → propagate, stop. result += {1}

Execution set = {1, 2, 3, 4, 5}.

Phase 3: Target (line 4) ∈ execution set → **Executed**.

### 6.5 Stacked annotations {#example-stacked}

```java
1:  public void foo() {                 Some({Declaration, ScopeOpen})
2:      //= spec.md#section-1          Some({Annotation})
3:      //# MUST do X                   Some({Annotation})
4:      //= spec.md#section-2          Some({Annotation})
5:      //# MUST do Y                   Some({Annotation})
6:      doXandY();                      Some({Statement})  ← coverage: Hit
7:  }                                   Some({ScopeClose})
```

Phase 1 for annotation A (lines 2-3): Walk forward → line 4 is Some({Annotation}) →
skip → line 5 is Some({Annotation}) → skip → line 6 is Some({Statement}) → target = line 6.

Phase 1 for annotation B (lines 4-5): Walk forward → line 6 is Some({Statement}) →
target = line 6.

Both annotations target line 6. Line 6 is Hit → both are **Executed**.

### 6.6 C code with goto (conservative fallback) {#example-goto}

```c
1:  void foo() {                        Some({Declaration, ScopeOpen})
2:      //= spec.md#section-1          Some({Annotation})
3:      //# MUST do X                   Some({Annotation})
4:      int x;                          Some({Declaration})
5:      goto skip;                      Some({NonLinearControl, Statement})  ← coverage: Hit
6:      do_x();                         Some({Statement})  ← coverage: Miss
7:  skip:                               Some({NonLinearControl})
8:      do_y();                         Some({Statement})  ← coverage: Hit
9:  }                                   Some({ScopeClose})
```

Phase 2: Scope (1-9) contains NonLinearControl (lines 5, 7) → **no
propagation**. Execution set = {5, 8} (only directly hit lines).

Phase 1: Annotation (lines 2-3) → target = line 4, properties: Some({Declaration}).

Phase 3: Target (line 4) ∉ execution set → **NotExecuted**.

This is the conservative fallback. Without `goto`, line 4 would have been in
the execution set via backward propagation from line 5. With `goto`, we can't
be sure line 4 was actually reached, so we don't propagate.

### 6.7 Unknown line blocks propagation {#example-unknown}

```java
1:  public void foo() {                 Some({Declaration, ScopeOpen})
2:      //= spec.md#section-1          Some({Annotation})
3:      //# MUST do X                   Some({Annotation})
4:      someUnrecognizedConstruct       None
5:      doX();                          Some({Statement})  ← coverage: Hit
6:  }                                   Some({ScopeClose})
```

Phase 1: Annotation (lines 2-3) → forward walk hits line 4 which is `None` →
target = line 4, properties: None.

Phase 3: Target properties are `None` → **Unknown**.

Note: Even though line 5 is executed, the unknown line at line 4 prevents the
annotation from being resolved to a known target. This is the conservative
behavior — unknown lines cannot produce false positives.

### 6.8 Unknown line blocks backward propagation {#example-unknown-propagation}

```java
1:  public void foo() {                 Some({Declaration, ScopeOpen})
2:      //= spec.md#section-1          Some({Annotation})
3:      //# MUST do X                   Some({Annotation})
4:      int x = 0;                      Some({Statement, Declaration})  ← coverage: Hit
5:      someUnrecognizedConstruct       None
6:      doX();                          Some({Statement})  ← coverage: Hit
7:  }                                   Some({ScopeClose})
```

Phase 1: Annotation (lines 2-3) → target = line 4, properties: Some({Statement, Declaration}).

Phase 2: Line 6 is Hit. Walk backward in scope (1-7):
- Line 5: None → **break** (unknown blocks propagation)

Line 4 is Hit. Walk backward in scope (1-7):
- Line 3: Some({Annotation}) → propagate. result += {3}
- Line 2: Some({Annotation}) → propagate. result += {2}
- Line 1: Some({ScopeOpen}) → propagate, stop. result += {1}

Execution set = {1, 2, 3, 4, 6}.

Phase 3: Target (line 4) ∈ execution set (directly hit) → **Executed**.

Note: The unknown line at line 5 blocks backward propagation from line 6, but
line 4 is directly hit so the annotation is still Executed. The unknown line
prevents propagation but does not invalidate direct hits.
