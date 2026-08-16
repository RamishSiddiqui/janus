/// Standalone test of rig-core with OpenRouter owl-alpha.
///
/// Usage: cargo run --bin test_owl -- <YOUR_OPENROUTER_API_KEY>
use futures::StreamExt;
use rig_core::providers::openrouter;
use rig_core::client::CompletionClient;
use rig_core::streaming::StreamingChat;
use rig_core::agent::MultiTurnStreamItem;
use rig_core::streaming::StreamedAssistantContent;
use rig_core::completion::Chat;

#[tokio::main]
async fn main() {
    let api_key = std::env::args().nth(1).expect("Usage: test_owl <API_KEY>");
    
    let client = openrouter::Client::new(&api_key)
        .expect("Failed to create OpenRouter client");

    println!("=== Test 1: Non-streaming chat ===");
    let agent = client.agent("openrouter/owl-alpha")
        .preamble("You are a helpful assistant.")
        .temperature(0.8)
        .build();

    let mut history = vec![];
    match agent.chat("Say hello in one sentence.", &mut history).await {
        Ok(resp) => println!("✅ Response: {resp}"),
        Err(e) => println!("❌ Error: {e}"),
    }

    println!("\n=== Test 2: Streaming chat ===");
    let agent2 = client.agent("openrouter/owl-alpha")
        .preamble("You are a helpful assistant.")
        .temperature(0.8)
        .build();

    let mut stream = agent2.stream_chat("Tell me a short joke.", Vec::<rig_core::completion::Message>::new()).await;
    {
        let mut full_text = String::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::Text(text)
                )) => {
                    print!("{}", text.text);
                    full_text.push_str(&text.text);
                }
                Ok(MultiTurnStreamItem::FinalResponse(fin)) => {
                    println!("\n✅ Final: {}", fin.response());
                    break;
                }
                Ok(other) => {
                    println!("[other item: {other:?}]");
                }
                Err(e) => {
                    println!("\n❌ Stream error: {e}");
                    break;
                }
            }
        }
        if full_text.is_empty() {
            println!("⚠️  No text received from stream");
        }
    }

    println!("\n=== Test 3: Streaming with long system prompt ===");
    let long_preamble = "You are Aria, a character in an immersive roleplay. Stay in character at all times. \
        Use vivid, descriptive prose with *actions* in asterisks. Never break the fourth wall. \
        Respond naturally to the user's actions and advance the narrative.\n\n\
        Character Description:\n\
        Daughter of a renowned elven mage and a human nobleman, Aria grew up surrounded by magic \
        but was never formally trained. She dreams of following in her mother's footsteps and proving \
        that half-elves can master the arcane arts just as well as any pure-blooded elf. Her natural \
        affinity for elemental magic is raw and untamed.\n\n\
        Personality:\n\
        Determined, curious, idealistic, sometimes reckless with magic, fiercely proud of her mixed \
        heritage, warm-hearted but quick to anger when her lineage is questioned.";

    let agent3 = client.agent("openrouter/owl-alpha")
        .preamble(long_preamble)
        .temperature(0.8)
        .build();

    let mut stream = agent3.stream_chat("*walks into the hall* Hello there.", Vec::<rig_core::completion::Message>::new()).await;
    {
        let mut full_text = String::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::Text(text)
                )) => {
                    print!("{}", text.text);
                    full_text.push_str(&text.text);
                }
                Ok(MultiTurnStreamItem::FinalResponse(fin)) => {
                    println!("\n✅ Final response received ({} chars)", fin.response().len());
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    println!("\n❌ Stream error: {e}");
                    break;
                }
            }
        }
        if full_text.is_empty() {
            println!("⚠️  No text received from stream");
        }
    }

    println!("\n=== Done ===");
}
