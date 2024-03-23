# Cargo Darwin

`cargo-darwin` is a plugin over `cargo` tool.

Darwin mutates your code, if your code still passes check tests, then your code isn't enough tested.

## Installation

```
cargo install cargo-darwin
```

## Usage

```bash
cargo darwin /path/to/project/to/test
```

Will display something like

```
[Missing] : Tests pass, the mutation hasn't been caught, suspicion of missing test
[OK]      : Tests failed, the mutation has been caught
[Timeout] : Mutation introduces infinite loop, inconclusive
[Killed]  : Mutation introduces non buildable modification
  ---
[OK] : Mutation #0 replace - by + in function "sub" of file src\a\toto.rs at line 11:6
[OK] : Mutation #1 replace - by * in function "sub" of file src\a\toto.rs at line 11:6
[Killed] : Mutation #2 replace - by && in function "sub" of file src\a\toto.rs at line 11:6
[Missing] : Mutation #3 replace + by - in function "add" of file src\lib.rs at line 5:6
[Missing] : Mutation #4 replace + by * in function "add" of file src\lib.rs at line 5:6
[Missing] : Mutation #5 replace + by - in function "add" of file src\lib.rs at line 5:10
[Missing] : Mutation #6 replace + by * in function "add" of file src\lib.rs at line 5:10
```

There is a `--dry-run` mode to just list mutation without actually apply tests.

```bash
cargo darwin --dry-run /path/to/project/to/test
```

## Details

*Darwin* walks the provided path (if none provided get the current dir).
For each file ending by **.rs** extension, **Darwin** analyze the file and try to found mutable
function.
A function is mutable if there is no `#[test]` or `#[tokio::test]` attribute over it.

```ignore
fn mutable() {}
#[test]
fn non_mutable() {}
#[tokio::test]
async fn non_mutable_async() {}
```

The project is in its really early stage, so the mutation are quite limited, actually just binary expressions
like `a + b` or `a - b`.
For example this mutable function

```
fn add(x: u8, y:u8) -> u8 {
    x + y
}
```

will become

```
fn add(x: u8, y:u8) -> u8 {
    x - y
}
```

Then Darwin create a copy of the actual project and apply the modification on the copied file
Once the project mutated, Darwin runs a `cargo build`, if the project compile, then the mutation is sustainable
If so, Darwin runs the `cargo test`, There are 3 possibilities:

- project tests pass : the project is inefficiently tested as the mutation isn't catch
- tests fail : the project has at least one test which catches the mutation
- timeout : the mutation even if compiles, introduce a loop or something that makes the test run forever

### Reports

All reports can be found in the *mutation path* in a **reports** folder which.
For example if you have run *darwin* with

```bash
cargo darwin --mutation-path /tmp/darwin /path/to/project/to/test
```

You will get this tree

```bash
tmp/
├─ darwin/
│  ├─ reports/
│  │  ├─ mutation_0.log
│  │  ├─ mutation_1.log
│  │  ├─ summary
│  ├─ 0/
│  ├─ 1/
```

#### Mutated projects

If the `--keep` flag is defined, after tests, you can walk to generated projects
Each one has a mutation ID and the associated mutation ID can be found in summary file

#### Summary

Summarize the mutation applied and the result of each.

```
[OK] : Mutation #0 replace - by + in function "sub" of file src\a\toto.rs at line 11:6
[OK] : Mutation #1 replace - by * in function "sub" of file src\a\toto.rs at line 11:6
[Killed] : Mutation #2 replace - by && in function "sub" of file src\a\toto.rs at line 11:6
[Missing] : Mutation #3 replace + by - in function "add" of file src\lib.rs at line 5:6
[Missing] : Mutation #4 replace + by * in function "add" of file src\lib.rs at line 5:6
[Missing] : Mutation #5 replace + by - in function "add" of file src\lib.rs at line 5:10
[Missing] : Mutation #6 replace + by * in function "add" of file src\lib.rs at line 5:10
```

For more information about the mutation, check the associated mutation_ID.log file

#### Mutation report

`reports/mutation_X.log` files are the detailed view of the mutation.
There are build with the following nomenclature

- Mutated file
- Mutation
- Mutation status
- Diff of the mutation
- Test or build output
  Below an example of output

```log
Mutation of file F:\Projets\Lab\Rust\darwin\playground\src\a\toto.rs
Mutation reason: replace - by *
Status : OK => Mutation Caught
Mutation diff:
@@ -8,7 +8,7 @@
 //
 fn sub(x: i8, y: i8) -> i8 {
     let u = 8;
-    x - y
+    x * y
 }
Output:
 #[test]
stderr:
   Compiling playground v0.1.0 (F:\Projets\Lab\Rust\darwin\tmp\1)
    Finished test [unoptimized + debuginfo] target(s) in 0.18s
     Running unittests src\lib.rs (target\debug\deps\playground-29148ab9d23d3c5d.exe)
error: test failed, to rerun pass `--lib`
stdout:
running 2 tests
test a::toto::test_sub ... FAILED
test a::toto::async_test_sub ... FAILED
failures:
---- a::toto::test_sub stdout ----
thread 'a::toto::test_sub' panicked at src\a\toto.rs:16:5:
assertion `left == right` failed
  left: 10
 right: 3
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
---- a::toto::async_test_sub stdout ----
thread 'a::toto::async_test_sub' panicked at src\a\toto.rs:21:5:
assertion `left == right` failed
  left: 10
 right: 3
failures:
    a::toto::async_test_sub
    a::toto::test_sub
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

As a test has failed, the mutation has been caught, so the code is enough tested for this particular mutation

## Limits

This project has done in the sole goal to understand the mutation testing and how it can be implemented.

That's why the set of mutation is hardcoded so far.

And very limited:

`a + b` gives:

- `a - b`
- `a * b`

`a - b` gives:

- `a + b`
- `a * b`
- `a && b`

## Trivia

Darwin stands for the "Natural selection law", the life mutates to adapt to environment so is doing cargo-darwin but
this time
the mutation are unwanted.

I would like to call it "Malcom" in honor of "Ian Malcom" in Jurassic Park, but the reference is too capillotracted ^^'