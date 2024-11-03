<p align=center>
    <a href=https://www.mapperinfluences.com>
    <img src=https://github.com/aticie/Mapper-Influences-Backend/assets/36697363/9386b5e7-bd1c-41f1-bb47-398cca2c7b6b>
    </a>
</p>
<p align=center>
    <a href=https://www.mapperinfluences.com>https://www.mapperinfluences.com</a>
</p>

---


Mapper influences backend code.
This is actually a rewrite of [this repository](https://github.com/aticie/Mapper-Influences-Backend). 

This implementation has more complete responses, optimizes osu! API calls and uses SurrealDB instead MongoDB as database.
I'm more comfortable with rust and strong types so that's going to make things easier for me going forward.

`/docs` for endpoint documentation.

If you have feature requests or bug reports, 
you can do so in [frontend repository](https://github.com/Fursum/mapper-influences-frontend) 
or in our [discord](https://discord.gg/SAwxBDe3Rf)
## How to run

#### Easiest way would be to use docker:
- Copy `.env.example` and change the name to `.env` 
- Fill it with your credentials.
- Use `docker compose up` to run the project..

You might only want to run database in docker, to do that just use `docker compose up surrealdb -d`

#### To run locally
`cargo run --release`

#### What is `conversion.rs` for?
It's a script to insert MongoDB data into SurrealDB. Don't use in production. I'm going to delete it after the migration is complete.

`cargo run --bin conversion`

### How to run tests
There isn't any tests yet so WIP 
