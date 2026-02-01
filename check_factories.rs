use ethers::prelude::*;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Provider::<Http>::try_from("https://testnet.risechain.org")?;
    
    let factory1 = Address::from_str("0x13b0D85CcB8bf860b6b79AF3029fCA081AE9beF2")?;
    let factory2 = Address::from_str("0x4e59b44847b379578588920ca78fbf26c0b4956c")?;
    
    let code1 = provider.get_code(factory1, None).await?;
    println!("Code at {:?}: {} bytes", factory1, code1.len());
    
    let code2 = provider.get_code(factory2, None).await?;
    println!("Code at {:?}: {} bytes", factory2, code2.len());
    
    Ok(())
}
