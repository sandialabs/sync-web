;; Imported from upstream s7test.scm line 1732.
;; Original form:
;; (test (eq? #||# (#|%%|# append #|^|#) #|?|# (#|+|# list #|<>|#) #||#) #t)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (eq? #||# (#|%%|# append #|^|#) #|?|# (#|+|# list #|<>|#) #||#))))
       (expected (upstream-safe (lambda () #t)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 1732 actual expected ok?))
