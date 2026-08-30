cpu management daemon

\- per-game profiles: scx scheduler, governor, turbo, freq, pl1 and pl2, applied on launch and restored on exit

\- cpu startup config, freq, pl1 and pl2 setter

config: `/etc/minazuki/config.toml` (auto reload)

```toml
[defaults]                 # any detected game with no [[game]] block
scheduler = "scx_lavd"
cpu_turbo = true

[[game]]                   # path is a substring of the command line
path = "Black Myth Wukong"
scheduler = "scx_cake"
cpu_pl1 = "150W"
```

for example config check [`config.toml`](data/config.example.toml)

```
git clone https://github.com/reakjra/minazuki && cd minazuki
make build
sudo make install
sudo systemctl daemon-reload
sudo systemctl enable --now minazuki
```
