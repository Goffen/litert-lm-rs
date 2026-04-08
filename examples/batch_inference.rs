use litert_lm::{Backend, Conversation, Engine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <model_path>", args[0]);
        std::process::exit(1);
    }
    let model_path = &args[1];

    println!("Loading model from: {}", model_path);

    let engine = Engine::new(model_path, Backend::Gpu)?;
    println!("Engine created successfully!\n");

    let prompts = vec![
        "What is the capital of France?",
        "Write a haiku about programming.",
        "What is 2 + 2?",
    ];

    println!("Running batch inference...\n");
    println!("========================================");

    for (i, prompt) in prompts.iter().enumerate() {
        println!("\n[{}] Prompt: {}", i + 1, prompt);

        // Each prompt gets a fresh conversation (no shared history).
        let mut convo = Conversation::new(&engine)?;

        match convo.send(prompt) {
            Ok(response) => {
                println!("Response: {}", response);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }

        println!("----------------------------------------");
    }

    println!("\nBatch inference complete!");

    Ok(())
}
