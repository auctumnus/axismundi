# tasks

the server has to do a couple kinds of tasks which range from "we don't want to block a request" to
"we don't want this to live in the main process because parsing is CVEful" to "we might want to retry
on failure".

in general, our structure is:

```mermaid
graph LR
    User([User])
    Proxy[Reverse Proxy<br/>nginx/caddy]
    Princess[Princess Process<br/>main app logic]
    DB[(Database)]
    Maid[Maid Processes<br/>sandboxed]
    DockerDaemon[Docker Daemon]
    External[External Services]
    User -->|requests| Proxy
    Proxy -->|forward| Princess
    DockerDaemon -->|health checks| Maid
    Princess -->|add tasks| DB
    Maid -->|poll tasks| DB
    Maid -->|work| External
    style Princess fill:#ff0066,color:#fff
    style Knight fill:#009edc,color:#fff
    style DockerDaemon fill:#fff,color:#000,stroke:#F8D3DAFF
```

- the reverse proxy takes in queries from users, does https, etc
- the princess process does all the "complicated logic", incl. routing
- the database stores user data, as well as tasks and references to maid processes
- maids do tasks

## the life of a maid

a maid is created by God (docker or whatever), and then goes to find her princess. she checks in with the database,
making a row for herself, and then starts 2 {tokio task, thread, something}s, a "work loop" and a "gossip loop".

for some timeout values N < M < L:

1. try to take a task
2. if there is no task, sleep for N seconds and goto 1
3. try to do the task
4. if you got an unrecoverable error but you still have database access, let the database know and die politely
5. if you got a recoverable error, let the database know it errored and goto 1
6. otherwise, you succeeded! let the database know it is complete and goto 1

```mermaid
flowchart TD
    Start([Start]) --> TakeTask[Try to take task]
    TakeTask --> DBCheck1{DB accessible?}
    DBCheck1 -->|No| Die([Die])
    DBCheck1 -->|Yes| HasTask{Task exists?}
    HasTask -->|No| Sleep[Sleep N seconds]
    Sleep --> TakeTask
    HasTask -->|Yes| DoTask[Do the task]
    DoTask --> CheckError{Error?}
    CheckError -->|Unrecoverable| ReportDie[Report to DB]
    ReportDie --> Die
    CheckError -->|Recoverable| ReportRetry[Report error to DB]
    ReportRetry --> DBCheck2{DB accessible?}
    DBCheck2 -->|No| Die
    DBCheck2 -->|Yes| TakeTask
    CheckError -->|Success| ReportSuccess[Report complete to DB]
    ReportSuccess --> DBCheck3{DB accessible?}
    DBCheck3 -->|No| Die
    DBCheck3 -->|Yes| TakeTask
    
    style Die fill:#c93c37
    style ReportSuccess fill:#428f46
    style ReportRetry fill:#daaa3f
```

## references

- https://web.archive.org/web/20250107140456/https://cohost.org/tef/post/1764930-how-not-to-write-a
- https://www.tumblr.com/swagophile/798333661074358272?source=share
- Puella Magi Madoka Magica. Directed by Akiyuki Shinbo, Shaft, 2011.
