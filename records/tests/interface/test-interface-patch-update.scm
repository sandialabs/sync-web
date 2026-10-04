(lambda (make-interface-harness)
  (with-let (make-interface-harness :journals 1 :legacy? #t)
    (define (error-result? result)
      (and (pair? result) (eq? (car result) 'error)))
    (define (collect action)
      (test-report)
      (test-submit action)
      (test-await))
    (define (snapshot)
      (raw journal-1
        '(*call* "pass-1"
          (lambda (root)
            (let ((ledger (sync-eval ((root 'get) '(root object ledger))))
                  (federation (sync-eval ((root 'get) '(root object federation)))))
              (list ((ledger 'config)) ((ledger 'signed-head) -1)
                    ((federation 'config))))))))
    (test-submit ((*journal* journal-1 'step!)) :expect integer?)
    (test-report)
    (let ((before (collect (snapshot))))
      (test-submit (update-interface journal-1 '() 4 #f)
        :expect "Installed interface")
      (test-submit (snapshot) :expect before)
      (test-submit
        (raw journal-1 '(*call* "pass-1"
          (lambda (root) ((root 'get) '(interface records-version)))))
        :expect "1.6.2")
      (test-report)
      (let ((installed (collect
               (raw journal-1 '(*call* "pass-1"
                 (lambda (root) (sync-digest *sync-state*)))))))
        (test-submit (update-interface journal-1 '() 4 #f)
          :expect error-result?)
        (test-submit
          (raw journal-1 '(*call* "pass-1"
            (lambda (root) (sync-digest *sync-state*))))
          :expect installed)))
    (test-report)))
