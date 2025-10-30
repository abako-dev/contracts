# Worker Ratings Contract

Smart contract to manage worker ratings in the Kunveno ecosystem.

## Features

- Register workers with an empty list of ratings (called by calendar)
- 0-100 rating system
- Stores a list of ratings per worker
- Calculates the average rating for each worker

## API

- `register_worker(worker)` - Calendar registers a worker with an empty rating list
- `add_rating(worker, rating)` - Projects add a rating for the worker (0-100)
- `get_worker_ratings(worker)` - Get the list of ratings for a worker
- `get_average_rating(worker)` - Get the average rating for a worker (returns 0 if there are no ratings)
- `get_registered_workers()` - Get a list of all workers
- `get_all_ratings()` - Get all workers with their ratings `Vec<(AccountId, Vec<u8>)>`

## Build

```bash
cargo contract build --release
```

## Testing

```bash
cargo test
```

## Usage

**Calendar** calls `register_worker()` when registering a worker  
**Projects** call `add_rating()` when a project is completed
