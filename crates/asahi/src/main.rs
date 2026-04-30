#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    asahi::rocket().launch().await?;
    Ok(())
}
