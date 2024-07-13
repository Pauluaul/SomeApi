pub mod database {
    use sqlx::{Connection, PgConnection};
    use sqlx::postgres::PgRow;

    pub const URL: &str = "postgres://postgres:Craul1308@localhost:5432/TestDatabase";

    async fn get_connection() -> PgConnection {
        match PgConnection::connect(URL)
            .await {
            Ok(conn) => conn,
            Err(error) => panic!("oh no: {}", error)
        }
    }

    async fn fetch(sql :String) -> PgRow {
        let result = match sqlx::query(&*sql)
            .fetch_one(&mut get_connection().await)
            .await {
            Ok(conn) => conn,
            Err(error) => panic!("oh no: {}", error)
        };
        return result
    }

    pub async fn perform_select(table_name :&str, param_names : [String; 0], param_values : [String; 0]) -> PgRow {
        let mut sql = " Select * from ".to_owned() + table_name;
        if param_names.len() != param_values.len() {
            panic!("param len diff")
        } else if param_names.len() > 0 {
            sql = sql + " where ";
        }

        for i in 0..param_names.len() {
            sql = sql + &param_names[i] + " = " + &param_values[i] + " ";
        }
        fetch(sql).await
    }
}