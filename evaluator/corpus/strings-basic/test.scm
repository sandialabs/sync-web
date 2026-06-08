(let ((s (copy "sync")))
  (set! (s 0) #\S)
  (list
    s
    (string-append s "-web")
    (substring "abcdef" 2 5)
    (length s)
    (string=? s "Sync")))
