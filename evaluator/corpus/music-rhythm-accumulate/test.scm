(define (onsets durations)
  (let loop ((ds durations) (time 0.0) (out '()))
    (if (null? ds)
        (reverse out)
        (loop (cdr ds) (+ time (car ds)) (cons time out)))))

(onsets '(0.25 0.25 0.5 1.0 0.125))
