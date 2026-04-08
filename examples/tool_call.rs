use litert_lm::{Backend, Conversation, Engine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <model_path>", args[0]);
        std::process::exit(1);
    }
    let model_path = &args[1];

    println!("Loading model from: {model_path}");
    let engine = Engine::new(model_path)?;
    println!("Engine created.\n");

    // Declare a tool the model can call.
    let tools = r#"[{
        "type": "function",
         "function": {
            "name": "get_weather",
            "description": "Returns the current weather for a given city.",
            "parameters": {
                "type": "object",
                "properties": {
                    "city": {
                        "type": "string",
                        "description": "The city name, e.g. Paris"
                        }
                    },
                "required": ["city"]
            }
         }
    }]"#;

    let mut convo = Conversation::with_config(&engine, None, Some(tools))?;
    println!("Conversation created with tool: hamta_vader\n");

    // Ask a question that should trigger the tool call.
    let prompt = "Vad är det för väder i Tokio just nu?";
    println!("User: {prompt}");
    let resp = convo.send(prompt)?;
    println!("Model response JSON:\n{}\n", resp.json());

    if resp.has_tool_calls() {
        println!("=> Model requested a tool call! Sending fake weather data back...\n");

        // Simulate executing the tool and returning a result.
        let tool_result = r#"{"role":"tool","content":[{
            "name": "get_weather",
            "response": {
                "city": "Tokyo",
                "temperatur_c": 18,
                "condition": "Partly cloudy",
                "humidity": 62
            }
        }]}"#;

        let final_resp = convo.send_json(tool_result)?;
        println!(
            "Model (after tool result):\n{}",
            final_resp
                .text()
                .unwrap_or_else(|| final_resp.json().to_string())
        );
    } else {
        println!(
            "Model answered directly (no tool call):\n{}",
            resp.text().unwrap_or_default()
        );
    }

    Ok(())
}
