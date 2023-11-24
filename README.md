# netns-ng

Manipulate network namespaces in Rust with ease.

## Usage

```rust
// get current ns
let cur_ns = Netns::get()?;

// get from named ns
let netns2 = Netns::get_from_name("test");

// unique id
println!("cur_ns: {}", cur_ns.unique_id());

// new ns
let new_netns = Netns::new()?;

// new named ns
let named_netns = Netns::new_named("test")?;

// set ns
let res = named_netns.set()?;

// del named ns
let res = Netns::delete_named("test");

// exec in other ns and return to current ns
let ns2 = Netns::get()?
let ns1 = Netns::new_named("test1")?;
exec_netns!(ns2, ns1, result, || -> anyhow::Result<()>{
    // do something in ns1
});
let res = result?;
```