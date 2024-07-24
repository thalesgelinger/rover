// THIS IS JUST A TEST FILE SHOULD BE DELETED LATER
use reqwest::Error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Define the URL of the file server
    let url = "http://127.0.0.1:4242/rover/lib/main.lua"; // Replace with your actual file path

    // Send the GET request
    let response = reqwest::get(url).await?;

    // Ensure the request was successful
    if response.status().is_success() {
        // Read the response body as text
        let content = response.text().await?;
        println!("File content: {}", content);
    } else {
        println!("Failed to fetch the file: {}", response.status());
    }

    Ok(())
}
