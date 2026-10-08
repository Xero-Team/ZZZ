/*!
A Rust interface for Objective-C blocks.

For more information on the specifics of the block implementation, see
Clang's documentation: http://clang.llvm.org/docs/Block-ABI-Apple.html

# Invoking blocks

The `Block` struct is used for invoking blocks from Objective-C. For example,
consider this Objective-C function:

``` objc
int32_t sum(int32_t (^block)(int32_t, int32_t)) {
    return block(5, 8);
}
```

We could write it in Rust as the following:

```
# use block::Block;
unsafe fn sum(block: &Block<(i32, i32), i32>) -> i32 {
    block.call((5, 8))
}
```

Note the extra parentheses in the `call` method, since the arguments must be
passed as a tuple.

# Creating blocks

Creating a block to pass to Objective-C can be done with the `ConcreteBlock`
struct. For example, to create a block that adds two `i32`s, we could write:

```
# use block::ConcreteBlock;
let block = ConcreteBlock::new(|a: i32, b: i32| a + b);
let block = block.copy();
assert!(unsafe { block.call((5, 8)) } == 13);
```

It is important to copy your block to the heap (with the `copy` method) before
passing it to Objective-C; this is because our `ConcreteBlock` is only meant
to be copied once, and we can enforce this in Rust, but if Objective-C code
were to copy it twice we could have a double free.
*/

use std::marker::PhantomData;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::os::raw::{c_int, c_ulong, c_void};
use std::ptr;

#[cfg_attr(
    any(target_os = "macos", target_os = "ios"),
    link(name = "System", kind = "dylib")
)]
#[cfg_attr(
    not(any(target_os = "macos", target_os = "ios")),
    link(name = "BlocksRuntime", kind = "dylib")
)]
extern "C" {
    static _NSConcreteStackBlock: c_void;

    fn _Block_copy(block: *const c_void) -> *mut c_void;
    fn _Block_release(block: *const c_void);
}

/// Types that may be used as the arguments to an Objective-C block.
pub trait BlockArguments: Sized {
    /// Calls the given `Block` with self as the arguments.
    ///
    /// # Safety
    ///
    /// `block` must point to a valid `Block`, and the caller must uphold the
    /// safety requirements of the foreign code invoked by that block.
    unsafe fn call_block<R>(self, block: *mut Block<Self, R>) -> R;
}

macro_rules! block_args_impl {
    ($($argument:ident : $argument_type:ident),*) => (
        impl<$($argument_type),*> BlockArguments for ($($argument_type,)*) {
            unsafe fn call_block<R>(self, block: *mut Block<Self, R>) -> R {
                let invoke: unsafe extern "C" fn(*mut Block<Self, R> $(, $argument_type)*) -> R = {
                    let base = block as *mut BlockBase<Self, R>;
                    mem::transmute((*base).invoke)
                };
                let ($($argument,)*) = self;
                invoke(block $(, $argument)*)
            }
        }
    );
}

block_args_impl!();
block_args_impl!(a: A);
block_args_impl!(a: A, b: B);
block_args_impl!(a: A, b: B, c: C);
block_args_impl!(a: A, b: B, c: C, d: D);
block_args_impl!(a: A, b: B, c: C, d: D, e: E);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J);
block_args_impl!(a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K);
block_args_impl!(
    a: A,
    b: B,
    c: C,
    d: D,
    e: E,
    f: F,
    g: G,
    h: H,
    i: I,
    j: J,
    k: K,
    l: L
);

#[repr(C)]
struct BlockBase<A, R> {
    isa: *const c_void,
    flags: c_int,
    reserved: c_int,
    invoke: unsafe extern "C" fn(*mut Block<A, R>, ...) -> R,
}

/// An Objective-C block that takes arguments of `A` when called and returns a
/// value of `R`.
#[repr(C)]
pub struct Block<A, R> {
    base: PhantomData<BlockBase<A, R>>,
}

impl<A: BlockArguments, R> Block<A, R> {
    /// Call self with the given arguments.
    ///
    /// # Safety
    ///
    /// The caller must ensure that invoking the foreign block does not violate
    /// Rust's safety rules. In particular, a shared block must not introduce a
    /// data race when called.
    pub unsafe fn call(&self, arguments: A) -> R {
        arguments.call_block(self as *const _ as *mut _)
    }
}

/// A reference-counted Objective-C block.
pub struct RcBlock<A, R> {
    pointer: *mut Block<A, R>,
}

impl<A, R> RcBlock<A, R> {
    /// Construct an `RcBlock` for the given block without copying it.
    /// The caller must ensure the block has a +1 reference count.
    ///
    /// # Safety
    ///
    /// `pointer` must point to a valid `Block` with a +1 reference count, or it
    /// will be overreleased when the `RcBlock` is dropped.
    pub unsafe fn new(pointer: *mut Block<A, R>) -> Self {
        RcBlock { pointer }
    }

    /// Constructs an `RcBlock` by copying the given block.
    ///
    /// # Safety
    ///
    /// `pointer` must point to a valid `Block`.
    pub unsafe fn copy(pointer: *mut Block<A, R>) -> Self {
        let pointer = _Block_copy(pointer as *const c_void) as *mut Block<A, R>;
        RcBlock { pointer }
    }
}

impl<A, R> Clone for RcBlock<A, R> {
    fn clone(&self) -> RcBlock<A, R> {
        unsafe { RcBlock::copy(self.pointer) }
    }
}

impl<A, R> Deref for RcBlock<A, R> {
    type Target = Block<A, R>;

    fn deref(&self) -> &Block<A, R> {
        unsafe { &*self.pointer }
    }
}

impl<A, R> Drop for RcBlock<A, R> {
    fn drop(&mut self) {
        unsafe {
            _Block_release(self.pointer as *const c_void);
        }
    }
}

/// Types that may be converted into a `ConcreteBlock`.
pub trait IntoConcreteBlock<A>: Sized
where
    A: BlockArguments,
{
    /// The return type of the resulting `ConcreteBlock`.
    type Ret;

    /// Consumes self to create a `ConcreteBlock`.
    fn into_concrete_block(self) -> ConcreteBlock<A, Self::Ret, Self>;
}

macro_rules! concrete_block_impl {
    ($function:ident) => (
        concrete_block_impl!($function,);
    );
    ($function:ident, $($argument:ident : $argument_type:ident),*) => (
        impl<$($argument_type,)* R, X> IntoConcreteBlock<($($argument_type,)*)> for X
        where
            X: Fn($($argument_type,)*) -> R,
        {
            type Ret = R;

            fn into_concrete_block(self) -> ConcreteBlock<($($argument_type,)*), R, X> {
                unsafe extern "C" fn $function<$($argument_type,)* R, X>(
                    block_pointer: *mut ConcreteBlock<($($argument_type,)*), R, X>
                    $(, $argument: $argument_type)*
                ) -> R
                where
                    X: Fn($($argument_type,)*) -> R,
                {
                    let block = &*block_pointer;
                    (block.closure)($($argument),*)
                }

                let function: unsafe extern "C" fn(
                    *mut ConcreteBlock<($($argument_type,)*), R, X>
                    $(, $argument_type)*
                ) -> R = $function;
                let invoke = unsafe {
                    mem::transmute::<
                        unsafe extern "C" fn(
                            *mut ConcreteBlock<($($argument_type,)*), R, X>
                            $(, $argument_type)*
                        ) -> R,
                        unsafe extern "C" fn(
                            *mut ConcreteBlock<($($argument_type,)*), R, X>,
                            ...
                        ) -> R,
                    >(function)
                };
                unsafe { ConcreteBlock::with_invoke(invoke, self) }
            }
        }
    );
}

concrete_block_impl!(concrete_block_invoke_args0);
concrete_block_impl!(concrete_block_invoke_args1, a: A);
concrete_block_impl!(concrete_block_invoke_args2, a: A, b: B);
concrete_block_impl!(concrete_block_invoke_args3, a: A, b: B, c: C);
concrete_block_impl!(concrete_block_invoke_args4, a: A, b: B, c: C, d: D);
concrete_block_impl!(concrete_block_invoke_args5, a: A, b: B, c: C, d: D, e: E);
concrete_block_impl!(concrete_block_invoke_args6, a: A, b: B, c: C, d: D, e: E, f: F);
concrete_block_impl!(concrete_block_invoke_args7, a: A, b: B, c: C, d: D, e: E, f: F, g: G);
concrete_block_impl!(concrete_block_invoke_args8, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H);
concrete_block_impl!(concrete_block_invoke_args9, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I);
concrete_block_impl!(concrete_block_invoke_args10, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J);
concrete_block_impl!(concrete_block_invoke_args11, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K);
concrete_block_impl!(concrete_block_invoke_args12, a: A, b: B, c: C, d: D, e: E, f: F, g: G, h: H, i: I, j: J, k: K, l: L);

/// An Objective-C block whose size is known at compile time and may be
/// constructed on the stack.
#[repr(C)]
pub struct ConcreteBlock<A, R, F> {
    base: BlockBase<A, R>,
    descriptor: Box<BlockDescriptor<ConcreteBlock<A, R, F>>>,
    closure: F,
}

impl<A, R, F> ConcreteBlock<A, R, F>
where
    A: BlockArguments,
    F: IntoConcreteBlock<A, Ret = R>,
{
    /// Constructs a `ConcreteBlock` with the given closure.
    /// When the block is called, it will return the value that results from
    /// calling the closure.
    pub fn new(closure: F) -> Self {
        closure.into_concrete_block()
    }
}

impl<A, R, F> ConcreteBlock<A, R, F> {
    /// Constructs a `ConcreteBlock` with the given invoke function and closure.
    /// Unsafe because the caller must ensure the invoke function takes the
    /// correct arguments.
    unsafe fn with_invoke(invoke: unsafe extern "C" fn(*mut Self, ...) -> R, closure: F) -> Self {
        ConcreteBlock {
            base: BlockBase {
                isa: &_NSConcreteStackBlock,
                flags: 1 << 25,
                reserved: 0,
                invoke: mem::transmute::<
                    unsafe extern "C" fn(*mut Self, ...) -> R,
                    unsafe extern "C" fn(*mut Block<A, R>, ...) -> R,
                >(invoke),
            },
            descriptor: Box::new(BlockDescriptor::new()),
            closure,
        }
    }
}

impl<A, R, F> ConcreteBlock<A, R, F>
where
    F: 'static,
{
    /// Copy self onto the heap as an `RcBlock`.
    pub fn copy(self) -> RcBlock<A, R> {
        unsafe {
            let mut block = self;
            let copied = RcBlock::copy(&mut *block);
            mem::forget(block);
            copied
        }
    }
}

impl<A, R, F> Clone for ConcreteBlock<A, R, F>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        let invoke = unsafe {
            mem::transmute::<
                unsafe extern "C" fn(*mut Block<A, R>, ...) -> R,
                unsafe extern "C" fn(*mut Self, ...) -> R,
            >(self.base.invoke)
        };
        unsafe { ConcreteBlock::with_invoke(invoke, self.closure.clone()) }
    }
}

impl<A, R, F> Deref for ConcreteBlock<A, R, F> {
    type Target = Block<A, R>;

    fn deref(&self) -> &Block<A, R> {
        unsafe { &*(&self.base as *const _ as *const Block<A, R>) }
    }
}

impl<A, R, F> DerefMut for ConcreteBlock<A, R, F> {
    fn deref_mut(&mut self) -> &mut Block<A, R> {
        unsafe { &mut *(&mut self.base as *mut _ as *mut Block<A, R>) }
    }
}

unsafe extern "C" fn block_context_dispose<B>(block: &mut B) {
    ptr::read(block);
}

unsafe extern "C" fn block_context_copy<B>(_destination: &mut B, _source: &B) {}

#[repr(C)]
struct BlockDescriptor<B> {
    reserved: c_ulong,
    block_size: c_ulong,
    copy_helper: unsafe extern "C" fn(&mut B, &B),
    dispose_helper: unsafe extern "C" fn(&mut B),
}

impl<B> BlockDescriptor<B> {
    fn new() -> BlockDescriptor<B> {
        BlockDescriptor {
            reserved: 0,
            block_size: mem::size_of::<B>() as c_ulong,
            copy_helper: block_context_copy::<B>,
            dispose_helper: block_context_dispose::<B>,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConcreteBlock;

    #[test]
    fn copied_block_invokes_closure() {
        let block = ConcreteBlock::new(|value: i32| value + 5).copy();

        assert_eq!(unsafe { block.call((6,)) }, 11);
    }
}
