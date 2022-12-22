pub mod serde_schemes {
    use serde::Deserialize;

    #[derive(Deserialize, Debug, Clone)]
    pub struct Variant {
        pub bit_rate: Option<i32>,
        pub content_type: String,
        pub url: String,
    }

    #[derive(Deserialize, Debug)]
    pub struct User {
        pub id_str: String,
        pub name: String,
        pub screen_name: String,
    }

    #[derive(Deserialize, Debug)]
    pub struct Media {
        pub r#type: String,
        pub preview_image_url: Option<String>,
        pub variants: Option<Vec<Variant>>,
        pub url: Option<String>,
    }

    #[derive(Deserialize, Debug)]
    pub struct TwitterUser {
        pub name: String,
        pub username: String,
    }

    #[derive(Deserialize, Debug)]
    pub struct MultimediaIncludes {
        pub media: Option<Vec<Media>>,
        pub users: Vec<TwitterUser>,
    }

    #[derive(Deserialize, Debug)]
    pub struct MultimediaData {
        pub text: Option<String>,
    }

    #[derive(Deserialize, Debug)]
    pub struct MultimediaBody {
        pub includes: Option<MultimediaIncludes>,
        pub data: MultimediaData,
    }
}
