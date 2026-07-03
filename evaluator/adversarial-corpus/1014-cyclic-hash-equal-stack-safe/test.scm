(equal? (let ((h (hash-table))) (set! (h 'self) h) h) (let ((h (hash-table))) (set! (h 'self) h) h))
