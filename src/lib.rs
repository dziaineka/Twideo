#[macro_use]
extern crate diesel;

pub mod models;
pub mod scheme;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use models::TLUser;

pub struct DBManager {
    connection: PgConnection,
}

type DieselResult<U> = std::result::Result<U, diesel::result::Error>;

impl DBManager {
    pub fn new(db_url: &str) -> Result<DBManager, diesel::result::ConnectionError> {
        Ok(DBManager {
            connection: PgConnection::establish(db_url)?,
        })
    }

    pub fn create_user(
        &self,
        id: i64,
        name_: String,
        username_: Option<String>,
    ) -> Result<(), diesel::result::Error> {
        use scheme::users;
        use scheme::users::dsl::*;

        let q: DieselResult<i64> = users
            .filter(chat_id.eq(id))
            .select(chat_id)
            .first(&self.connection);

        if q.is_ok() {
            return Ok(());
        }

        let user = TLUser {
            chat_id: id,
            name: name_,
            username: username_,
        };

        let row: DieselResult<TLUser> = diesel::insert_into(users::table)
            .values(&user)
            .get_result(&self.connection);

        match row {
            Ok(result) => {
                log::info!("User Created Successfully => {:?}", result);
                Ok(())
            }
            Err(error) => {
                log::error!("User Creation Failed => {:?}", error);
                Err(error)
            }
        }
    }
}

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
