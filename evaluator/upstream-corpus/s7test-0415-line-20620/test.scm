;; Imported from upstream s7test.scm line 20620.
;; Original form:
;; (test (let-temporarily (((current-output-port) #f)) (port-closed? (current-output-port))) #f)

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (let-temporarily (((current-output-port) #f)) (port-closed? (current-output-port))))))
       (expected (upstream-safe (lambda () #f)))
       (ok? (equal? actual expected)))
  (list 'upstream-test 20620 actual expected ok?))
