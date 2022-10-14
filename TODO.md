# F/E

[ ] 2 fields for Alpaca API keys
[ ] List open and closed positions
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

[ ] top row blank fields for sym, qty, limit, stop, target
[ ] exec button if pending
[ ] liquidate if open
[ ] edit stop/target if open
[ ] page results
[ ] filter by date, symbol, etc

# B/E

  1. [x] GET /quote?sym=AAPL => { last: 129.00, bid: 129.34, ask: 129.44 }
  2. [x] POST /order: (per above)
  3. [x] GET /positions: List positions via Alpaca (directly?)
  4. [x] GET /orders: List open orders via Alpaca (directly?)
  5. [ ] PATCH /order: modify stop, target
  6. [ ] POST /monitor: symbol and period, add to list, then thread-stream live trades/quotes every period via w/s
  7. [ ] POST /watch: Strategy ID and symbol

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
  * ta-rs
  * diesel
  * axum

Example trade-bot: https://github.com/Nukeuler123/trade-bot/

[Rust on Nails](https://rust-on-nails.com/): interesting, seems unfinished in
RE to F/E, authentication, RBAC. But some interesting ideas.

