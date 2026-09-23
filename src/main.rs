use std::{env, error::Error, path::Path};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "mcp") && args.len() <= 3 {
        use rmcp::ServiceExt;
        // Explicit argument wins, then environment override, then local default.
        // The default is relative to the server's working directory.
        let path = args
            .get(2)
            .cloned()
            .or_else(|| env::var("ASPECTWRITE_STROKES").ok())
            .unwrap_or_else(|| ".local/aspectwrite-strokes.json".into());
        let server = aspectwrite::server::AspectServer::new(Path::new(&path))?;
        server
            .serve(rmcp::transport::stdio())
            .await?
            .waiting()
            .await?;
        return Ok(());
    }
    if args.len() < 5 || args.len() > 7 || args[1] != "render" {
        eprintln!(
            "Usage: aspectwrite render <strokes.json> <output.png> '<LaTeX>' [--seed <u64>]\n       aspectwrite mcp [strokes.json]  (defaults to .local/aspectwrite-strokes.json)"
        );
        std::process::exit(2);
    }
    let seed = if args.len() == 7 && args[5] == "--seed" {
        args[6].parse::<u64>()?
    } else if args.len() == 5 {
        0
    } else {
        return Err("expected --seed <u64>".into());
    };
    let handwriting = aspectwrite::render::Handwriting::load(Path::new(&args[2]))?;
    let png = aspectwrite::render_png_with_seed(&args[4], &handwriting, seed)?;
    std::fs::write(&args[3], png)?;
    eprintln!("Wrote {}", args[3]);
    Ok(())
}
