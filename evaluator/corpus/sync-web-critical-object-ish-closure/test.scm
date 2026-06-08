(define (make-cell value)
  (let ((state (inlet 'value value)))
    (lambda (msg . args)
      (case msg
        ((get) (state 'value))
        ((set!) (set! (state 'value) (car args)) #t)
        ((state) state)
        (else (error 'method-error "unknown method: ~S" msg))))))

(let ((cell (make-cell 1)))
  (cell 'set! 42)
  (list
    (list 'value (cell 'get))
    (list 'state-value ((cell 'state) 'value))))
