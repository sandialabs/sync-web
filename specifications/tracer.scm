;; wraps objects with operation, arguments, success/error, and returned value

(begin




  
(define (trace-sanitize-string s)
  s)


;; turns values into logs so every arg and result can be properly read  back
(define (trace-safe x)
  (cond ((sync-node? x) '(opaque sync-node))
        ((procedure? x) '(opaque procedure))
        ((string? x) (trace-sanitize-string x))
        ((pair? x) (cons (trace-safe (car x)) (trace-safe (cdr x))))
        (else (catch #t
                     (lambda () (object->string x) x)
                     (lambda args '(opaque unprintable))))))

  (define trace-seq 0)

    (define (trace-next-seq!)
        (set! trace-seq (+ trace-seq 1))
        trace-seq)



;; Wraps an object so method calls are logged as events
;; Calls that are not modeled are passed through 
  (define (trace-wrap obj journal-name log!)
    (lambda args
      (if (or (null? args) (not (symbol? (car args))))
          (apply obj args)
          (let* ((method (car args))
                 (call-args (cdr args)))
            (catch #t
              (lambda ()
                (let ((result (apply obj args)))
                  (if (procedure? result)
                      (lambda inner-args
                        (let ((seq (trace-next-seq!)))
                          (catch #t
                            (lambda ()
                              (let ((final (apply result inner-args)))
                                (log! (list 'event seq journal-name method
                                            (trace-safe inner-args) 'ok (trace-safe final)))
                                final))
                            (lambda error-args
                              (log! (list 'event seq journal-name method
                                          (trace-safe inner-args) 'error (trace-safe error-args)))
                              (apply error error-args)))))
                      (let ((seq (trace-next-seq!)))
                        (log! (list 'event seq journal-name method
                                    (trace-safe call-args) 'ok (trace-safe result)))
                        result))))
              (lambda error-args
                (let ((seq (trace-next-seq!)))
                  (log! (list 'event seq journal-name method
                              (trace-safe call-args) 'error (trace-safe error-args)))
                  (apply error error-args))))))))




;; turns collected list into string of event records 
;; log: ordered events for trace arrangements 
  (define (trace-serialize log)
    (let loop ((log log) (acc ""))
      (if (null? log) acc
          (loop (cdr log) (string-append acc (object->string (car log)) "\n"))))))