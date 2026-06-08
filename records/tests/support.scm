(begin

  (define asserted 0)

  (define (trace) (stacktrace 20 120 180 120 #f))

  (define-macro (assert expression expected)
    `(let* ((render (lambda (x)
                      (catch #t
                             (lambda () (object->string x))
                             (lambda args "<unprintable>"))))
            (trunc (lambda (x y) (if (< (length x) y) x (append (substring x 0 y) " ...")))))
       (catch #t
              (lambda ()
                (let* ((result~ ,expression)
                       (expected~ ,expected)
                       (check~ (cond ((not expected~) (lambda (x) #t))
                                     ((procedure? expected~) expected~)
                                     (else (lambda (result) (equal? result expected~))))))
                  (if (check~ result~)
                      (begin (set! asserted (+ asserted 1)) result~)
                      (error 'assertion-failure
                             (append "[Check " (object->string asserted) " failed] "
                                     "[Expression " (render ',expression) "] "
                                     "[Evaluated " (trunc (render result~) 256) "] "
                                     "[Expected " (trunc (render expected~) 256) "]\n"
                                     "[Stacktrace\n" (trace) "]")))))
              (lambda args
                (error 'assertion-failure
                       (append "[Check " (object->string asserted) " errored] "
                               "[Expression " (render ',expression) "] "
                               "[Error " (trunc (render args) 256) "]\n"
                               "[Stacktrace\n" (trace) "]")))))))
