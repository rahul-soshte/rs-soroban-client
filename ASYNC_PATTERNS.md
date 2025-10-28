# Using Account in Async Contexts

This guide explains how to use the `Account` type in async Rust applications.

## TL;DR

**For async contexts where you need to share an `Account` across tasks:**

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

let account = Account::new(account_id, sequence)?;
let shared_account = Arc::Mutex::new(account);

// Now you can clone and share across async tasks
let account_clone = shared_account.clone();
tokio::spawn(async move {
    let mut guard = account_clone.lock().await;
    let tx = TransactionBuilder::new(&mut *guard, network, None)
        .fee(1000u32)
        .build();
});
```

## Understanding the Design

### Why `&mut Account`?

The `TransactionBuilder` takes `&mut Account` because it needs to increment the account's sequence number when building a transaction. This design:

1. **Prevents race conditions at compile time** - You can't build two transactions simultaneously from the same account
2. **Ensures correct sequence numbers** - Sequence numbers are automatically managed
3. **Makes the API simple** - No need to manually track sequence numbers

### The Challenge with Async

In async contexts, you might want to:
- Share an account across multiple async tasks
- Build transactions from different parts of your application
- Hold an account in a struct that needs to be `Send + Sync`

The `&mut Account` pattern works perfectly, but you need to wrap it properly for sharing.

## Pattern 1: Simple Sequential Usage (Recommended)

**When to use**: You're building transactions one after another in the same async function.

```rust
async fn build_transactions() -> Result<(), Error> {
    let mut account = Account::new(account_id, sequence)?;

    // Build first transaction
    let tx1 = TransactionBuilder::new(&mut account, network, None)
        .fee(1000u32)
        .build();

    // Do async work (account is not borrowed here)
    server.send_transaction(tx1).await?;

    // Build second transaction
    let tx2 = TransactionBuilder::new(&mut account, network, None)
        .fee(1000u32)
        .build();

    server.send_transaction(tx2).await?;

    Ok(())
}
```

**Pros**:
- Simple and straightforward
- No overhead
- Compiler ensures correctness

**Cons**:
- Can't share account across tasks

## Pattern 2: Shared with `Arc<Mutex<Account>>`

**When to use**: You need to share an account across multiple async tasks or store it in a struct.

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

struct TransactionService {
    account: Arc<Mutex<Account>>,
}

impl TransactionService {
    async fn build_transaction(&self) -> Result<Transaction, Error> {
        // Lock the account (blocks other tasks from using it)
        let mut account = self.account.lock().await;

        // Build transaction (increments sequence)
        let tx = TransactionBuilder::new(&mut *account, network, None)
            .fee(1000u32)
            .build();

        Ok(tx)
        // Lock is released here
    }
}
```

**Pros**:
- Can share across tasks and async boundaries
- Thread-safe
- Still ensures correct sequence numbers

**Cons**:
- Slight overhead from mutex
- Tasks will wait if account is locked

## Pattern 3: Multiple Accounts for Parallelism

**When to use**: You need true parallelism for building transactions.

```rust
async fn parallel_transactions() -> Result<(), Error> {
    // Use different accounts for true parallelism
    let mut account1 = Account::new(pubkey1, seq1)?;
    let mut account2 = Account::new(pubkey2, seq2)?;

    let (tx1, tx2) = tokio::join!(
        build_tx(&mut account1),
        build_tx(&mut account2),
    );

    Ok(())
}
```

**Pros**:
- True parallelism
- No locking overhead

**Cons**:
- Requires multiple accounts

## Comparison with `Rc<RefCell<Account>>`

### Old Pattern (Rc<RefCell<>>)

```rust
// ❌ Doesn't work in async contexts
let account = Rc::new(RefCell::new(Account::new(id, seq)?));
// Error: Rc<RefCell<>> is not Send, can't use across .await
```

**Problems**:
- `Rc` is not `Send` - can't be shared across threads
- Runtime panics if you try to borrow mutably twice
- No compile-time safety

### New Pattern (&mut Account)

```rust
// ✅ Works great - compile-time safety
let mut account = Account::new(id, seq)?;
TransactionBuilder::new(&mut account, network, None)

// ✅ For sharing, wrap in Arc<Mutex<>>
let account = Arc::new(Mutex::new(Account::new(id, seq)?));
```

**Benefits**:
- Compile-time safety - can't make mistakes
- Works in async contexts with `Arc<Mutex<>>`
- Clear ownership semantics
- Works across thread boundaries

## Best Practices

1. **Default to Pattern 1** (simple sequential) unless you need sharing
2. **Use Pattern 2** when you need to share across async tasks or store in a struct
3. **Use Pattern 3** when you need true parallelism
4. **Don't hold locks across `.await` unnecessarily** - build the transaction and release the lock before async operations

## Examples

See the full working examples:
- [examples/async_patterns.rs](examples/async_patterns.rs) - Basic patterns
- [examples/async_real_world.rs](examples/async_real_world.rs) - Real-world service pattern
- [examples/deploy.rs](examples/deploy.rs) - Simple sequential pattern

## Why This Design is Good

The `&mut Account` design might seem limiting at first, but it actually provides:

1. **Compile-time safety** - Impossible to create race conditions
2. **Ergonomic API** - Simple sequential usage is the easiest path
3. **Flexible for advanced use** - Arc<Mutex<>> provides sharing when needed
4. **Idiomatic Rust** - Uses Rust's ownership system as intended

This is much better than the old `Rc<RefCell<>>` pattern, which:
- Panics at runtime instead of compile-time errors
- Doesn't work across async boundaries
- Isn't thread-safe
