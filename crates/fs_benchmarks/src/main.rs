use fs::Fs;
use gpui::AppContext;
use gpui_platform::headless;

fn main() {
    let Some(path_to_read) = std::env::args().nth(1) else {
        println!("Expected path to read as 1st argument.");
        return;
    };

    headless().run(|cx| {
        let fs = fs::RealFs::new(None, cx.background_executor().clone());
        cx.background_spawn(async move {
            let timer = std::time::Instant::now();
            match fs.load_bytes(path_to_read.as_ref()).await {
                Ok(bytes) => {
                    println!("Took {:?} to read {} bytes", timer.elapsed(), bytes.len())
                }
                Err(error) => println!(
                    "Failed `load_bytes` after {:?} with error `{error}`",
                    timer.elapsed()
                ),
            }

            let timer = std::time::Instant::now();
            match fs.metadata(path_to_read.as_ref()).await {
                Ok(_) => println!("Took {:?} to query metadata", timer.elapsed()),
                Err(error) => println!(
                    "Failed `metadata` after {:?} with error `{error}`",
                    timer.elapsed()
                ),
            }

            std::process::exit(0);
        })
        .detach();
    });
}
