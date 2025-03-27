#!/bin/bash

# Run the server in the background without opening a terminal
cargo run > /dev/null 2>&1 &
SERVER_PID=$!

# Open a new terminal for the client
gnome-terminal -- bash -c "cd client; cargo run; exec bash"

wait

kill $SERVER_PID
