# F/E

[ ] 2 fields for Alpaca API keys
[x] List open and closed positions
   - symbol
   - qty
   - date open
   - limit *
   - filled avg price
   - cost basis (incl fee)
   - stop *
   - target *
   - elevation, etc
   - current

* soft or hard

If closed
   - sold at
   - filled avg price
   - proceeds
   - % g/li
   - slip %

[x] new order fields for sym, qty, limit, stop, target
[ ] exec button if pending
[x] liquidate if open
[ ] edit stop/target if open
[ ] page results
[ ] filter by date, symbol, etc
[ ] performance and bracket charts
[ ] news
[ ] last n trades (stream)
[ ] history
[ ] manual order entry
[ ] alerts/notifications
[ ] bucket portfolios (by broker, IRA vs play, etc)

# B/E

  1. [x] GET /quote?sym=AAPL => { last: 129.00, bid: 129.34, ask: 129.44 }
  2. [x] POST /order: (per above)
  3. [x] GET /positions: List positions via Alpaca (directly?)
  4. [x] GET /orders: List open orders via Alpaca (directly?)
  5. [ ] PATCH /order: modify stop, target
  5. [x] PATCH /liquidate: cancel outstanding legs and enter a sell
  6. [ ] POST /monitor: symbol and period, add to list, then thread-stream live trades/quotes every period via w/s
  7. [ ] POST /watch: Strategy ID and symbol

[x] Bracket order impl https://alpaca.markets/docs/trading/orders/#bracket-orders
[x] Extend apca to get latest trade (not quote) https://alpaca.markets/docs/api-references/market-data-api/stock-pricing-data/historical/#latest-trade
[ ] Bucket orders by strategy/broker/etc
[ ] Order manual entry
[ ] OAuth to Alpaca

## TODO
[x] Shorting - does not work with bracket orders, getting `{ code: 42210000, message: "bracket orders must be entry orders" }` when trying to sell unheld stock
[x] clean up 404ing orders on startup_sync
[ ] splits!

Place order wf:
 1. submit values
 2. create Lot
 3. send to Alpaca
 4. fill lot with order deets
 5. poll/ws for order updates (fuk!)

on startup:
 1. fetch any non-finalized status orders and update Lots
 2. subscribe to order updates stream (and proxy!?)

Algo trading is a crowded and technically advanced market. Don't try to implement TradeStation, UltraAlgo etc

Focus: manual position entry and monitoring for long-term, family office etc,
on Alpaca. Uncomplicated and secure. Enable pouncing on good entry/exit via
live feed and short-term directional strength

Then: sparklines

Then: News

Next: TD Ameritrade, Interactive Brokers

Eventual: feed alerts and autofill input from 3rd party algo runners (somehow, depending on scrape or api avail)

## Crates
  * apca
  * [ta-rs](https://github.com/greyblake/ta-rs)
  * diesel
  * axum

[Example trade-bot](https://github.com/Nukeuler123/trade-bot/)

[Rust on Nails](https://rust-on-nails.com/): interesting, seems unfinished in
RE to F/E, authentication, RBAC. But some interesting ideas.

[Backend API w Rust on Postgres](https://blog.logrocket.com/create-backend-api-with-rust-postgres/)

[Diesel ORM](https://diesel.rs/)

[Turbosql](https://github.com/trevyn/turbosql)

[Structsy](https://www.structsy.rs/)

## Rust

https://blog.jcoglan.com/2019/04/22/generic-returns-in-rust/

