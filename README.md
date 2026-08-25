cpu management daemon

\- per-process scx scheduler loader

\- cpu startup config, freq, pl1 and pl2 setter

config: `/etc/minazuki/config.toml`

```
git clone https://github.com/reakjra/minazuki && cd minazuki
make build
sudo make install
sudo systemctl daemon-reload
sudo systemctl enable --now minazuki
```
