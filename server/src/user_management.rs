use crate::auth::{FullUser, StaySignedInToken};
use crate::{set_up_db, Config, UserCommand};
use isixhosa_common::database::db_impl::DbImpl;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tabled::Table;

pub fn run_command(cfg: Config, command: UserCommand) -> anyhow::Result<()> {
    let manager = SqliteConnectionManager::file(cfg.database_path);
    let pool = Pool::new(manager)?;
    set_up_db(&*pool.get()?)?;
    let db = DbImpl(pool);

    match command {
        UserCommand::SetRole { user, role } => {
            let modified = FullUser::set_role_by_email(&db, user, role);

            if modified {
                println!("User set to {role}");
            } else {
                println!("No changes made");
            }
        }
        UserCommand::Lock { user } => {
            let modified = FullUser::set_locked_by_email(&db, user, true);

            if modified {
                println!("User locked");
            } else {
                println!("No changes made");
            }
        }
        UserCommand::Unlock { user } => {
            let modified = FullUser::set_locked_by_email(&db, user, false);

            if modified {
                println!("User unlocked");
            } else {
                println!("No changes made");
            }
        }
        UserCommand::List => {
            println!("{}", Table::new(FullUser::fetch_all(&db)));
        }
        UserCommand::LogoutAll => {
            StaySignedInToken::delete_all(&db);
            println!("Logged out all users")
        }
    }

    Ok(())
}
