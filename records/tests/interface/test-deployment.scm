(lambda (make-interface-harness)

  ;; Compose renders INTERFACE_ADMINS as literal principal data. Verify that a
  ;; fresh install recognizes it immediately and that normal admin replacement
  ;; preserves the same principal shape.
  (with-let (make-interface-harness
             :journals 1
             :users '(admin alice)
             :admins '((*state* admin)))

    (define (error-result? value)
      (and (pair? value) (eq? (car value) 'error)))

    (test-submit ((admin journal-1 '*admins-get*))
                 :expect '((*state* admin)))
    (test-submit ((admin journal-1 '*admins-set*)
                  '((admins ((*state* alice)))))
                 :expect #t)
    (test-submit ((alice journal-1 '*admins-get*))
                 :expect '((*state* alice)))
    (test-submit ((admin journal-1 '*admins-get*))
                 :expect error-result?)
    (test-report)))
